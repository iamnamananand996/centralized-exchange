use crate::order_book::types::OrderStatus;
use crate::order_book::{
    db_persistence::DbPersistence, position_tracker::PositionTracker,
    redis_persistence::RedisOrderBookPersistence, Order, OrderSide, OrderType, TimeInForce,
};
use crate::types::order_book::{
    CancelOrderRequest, MarketDepthResponse, OrderBookResponse, OrderResponse, PlaceOrderRequest,
    PlaceOrderResponse, TradeResponse,
};
use crate::utils::cache::{cache_keys, create_cache_key, CacheService};
use crate::websocket::server::WebSocketServer;
use actix::Addr;
use actix_web::{web, Error, HttpResponse, Result};
use deadpool_redis::Pool;
use entity::{event_options, events, users};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set, TransactionTrait};
use serde_json::json;

// Removed static ORDER_BOOKS - now using Redis for all order book storage

pub async fn place_order(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    req: web::Json<PlaceOrderRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    // Initialize persistence layers
    let redis_persistence = RedisOrderBookPersistence::new(redis_pool.get_ref().clone());
    let db_persistence = DbPersistence::new(db.get_ref().clone());
    let position_tracker = PositionTracker::new(db.get_ref().clone());

    // Validate event exists and is active
    let event = events::Entity::find_by_id(req.event_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let event = match event {
        Some(e) => e,
        None => {
            return Ok(HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": "Event not found"
            })));
        }
    };

    // Check if event is active and not ended
    if event.status != "active" {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": "Event is not active for trading"
        })));
    }

    if event.end_time <= chrono::Utc::now().naive_utc() {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": "Event has already ended"
        })));
    }

    // Validate option exists and belongs to event
    let option = event_options::Entity::find_by_id(req.option_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let option = match option {
        Some(o) => o,
        None => {
            return Ok(HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": "Option not found"
            })));
        }
    };

    if option.event_id != req.event_id {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": "Option does not belong to the specified event"
        })));
    }

    // Get user's current balance
    let user = users::Entity::find_by_id(user_id_int)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let user = match user {
        Some(u) => u,
        None => {
            return Ok(HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": "User not found"
            })));
        }
    };

    // Validate based on order side
    match req.side {
        OrderSide::Buy => {
            // Check balance for buy orders
            let required_amount = req.price * sea_orm::prelude::Decimal::from(req.quantity);
            if user.wallet_balance < required_amount {
                return Ok(HttpResponse::BadRequest().json(json!({
                    "success": false,
                    "message": "Insufficient balance"
                })));
            }
        }
        OrderSide::Sell => {
            // Check position for sell orders
            let has_shares = position_tracker
                .validate_sell_order(user_id_int, req.event_id, req.option_id, req.quantity)
                .await
                .map_err(|e| {
                    log::error!("Position validation error: {}", e);
                    actix_web::error::ErrorInternalServerError("Failed to validate position")
                })?;

            if !has_shares {
                return Ok(HttpResponse::BadRequest().json(json!({
                    "success": false,
                    "message": "Insufficient shares to sell"
                })));
            }
        }
    }

    // Create the order
    let time_in_force = req.time_in_force.clone().unwrap_or(TimeInForce::GTC);
    let order = Order::new(
        user_id_int,
        req.event_id,
        req.option_id,
        req.side.clone(),
        req.order_type.clone(),
        time_in_force.clone(),
        req.price,
        req.quantity,
    );

    let order_id = order.id.clone();

    // Save order to database first
    if let Err(e) = db_persistence.save_order(&order).await {
        log::error!("Failed to save order to database: {}", e);
        return Err(actix_web::error::ErrorInternalServerError(
            "Failed to save order",
        ));
    }

    // Save order to Redis
    if let Err(e) = redis_persistence.save_order(&order).await {
        log::error!("Failed to save order to Redis: {}", e);
    }

    // Get or create order book from Redis
    let mut order_book = redis_persistence
        .get_or_create_order_book(req.event_id, req.option_id)
        .await
        .map_err(|e| {
            log::error!("Failed to get order book from Redis: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to get order book")
        })?;

    // Submit order to order book
    let trades = match order_book.submit_order(order) {
        Ok(trades) => trades,
        Err(e) => {
            log::error!("Order submission error: {}", e);
            // Update order status as rejected in database
            let rejected_order = Order {
                id: order_id.clone(),
                user_id: user_id_int,
                event_id: req.event_id,
                option_id: req.option_id,
                side: req.side.clone(),
                order_type: req.order_type.clone(),
                time_in_force,
                price: req.price,
                quantity: req.quantity,
                filled_quantity: 0,
                status: OrderStatus::Rejected,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let _ = db_persistence.update_order(&rejected_order).await;
            return Ok(HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": e
            })));
        }
    };

    // Save updated order book back to Redis
    if let Err(e) = redis_persistence
        .save_full_order_book(req.event_id, req.option_id, &order_book)
        .await
    {
        log::error!("Failed to save order book to Redis: {}", e);
    }

    // Process trades in a database transaction
    let updated_balance = if !trades.is_empty() {
        let txn = db.get_ref().begin().await.map_err(|e| {
            log::error!("Failed to start transaction: {}", e);
            actix_web::error::ErrorInternalServerError("Transaction error")
        })?;

        let mut current_balance = user.wallet_balance;

        for trade in &trades {
            // Validate seller has shares before processing the trade
            let seller_has_shares = match position_tracker
                .validate_sell_order(
                    trade.seller_id,
                    trade.event_id,
                    trade.option_id,
                    trade.quantity,
                )
                .await
            {
                Ok(has_shares) => has_shares,
                Err(e) => {
                    log::error!("Position validation error: {}", e);
                    let _ = txn.rollback().await;
                    return Err(actix_web::error::ErrorInternalServerError(
                        "Failed to validate position",
                    ));
                }
            };

            if !seller_has_shares {
                log::error!(
                    "Seller {} doesn't have enough shares for trade {}",
                    trade.seller_id,
                    trade.id
                );
                let _ = txn.rollback().await;
                return Err(actix_web::error::ErrorInternalServerError(
                    "Trade execution failed: seller has insufficient shares",
                ));
            }

            // Save trade to database
            if let Err(e) = db_persistence.save_trade(trade).await {
                log::error!("Failed to save trade to database: {}", e);
                let _ = txn.rollback().await;
                return Err(actix_web::error::ErrorInternalServerError(
                    "Failed to save trade",
                ));
            }

            // Save trade to Redis
            if let Err(e) = redis_persistence.save_trade(trade).await {
                log::error!("Failed to save trade to Redis: {}", e);
            }

            // Update positions
            if let Err(e) = position_tracker.update_positions_from_trade(trade).await {
                log::error!("Failed to update positions: {}", e);
                let _ = txn.rollback().await;
                return Err(actix_web::error::ErrorInternalServerError(
                    "Failed to update positions",
                ));
            }

            // Update user balances in the database
            // Update buyer's balance (decrease)
            let buyer = users::Entity::find_by_id(trade.buyer_id)
                .one(&txn)
                .await
                .map_err(|e| {
                    log::error!("Failed to find buyer: {}", e);
                    actix_web::error::ErrorInternalServerError("Database error")
                })?
                .ok_or_else(|| actix_web::error::ErrorInternalServerError("Buyer not found"))?;

            let mut active_buyer: users::ActiveModel = buyer.into();
            let new_buyer_balance = active_buyer.wallet_balance.as_ref() - trade.total_amount;
            if new_buyer_balance < sea_orm::prelude::Decimal::new(0, 2) {
                let _ = txn.rollback().await;
                return Err(actix_web::error::ErrorBadRequest(
                    "Insufficient buyer balance",
                ));
            }
            active_buyer.wallet_balance = Set(new_buyer_balance);
            active_buyer.updated_at = Set(chrono::Utc::now().naive_utc());
            if let Err(e) = active_buyer.update(&txn).await {
                log::error!("Failed to update buyer balance: {}", e);
                let _ = txn.rollback().await;
                return Err(actix_web::error::ErrorInternalServerError(
                    "Failed to update balance",
                ));
            }

            // Update seller's balance (increase)
            let seller = users::Entity::find_by_id(trade.seller_id)
                .one(&txn)
                .await
                .map_err(|e| {
                    log::error!("Failed to find seller: {}", e);
                    actix_web::error::ErrorInternalServerError("Database error")
                })?
                .ok_or_else(|| actix_web::error::ErrorInternalServerError("Seller not found"))?;

            let mut active_seller: users::ActiveModel = seller.into();
            let new_seller_balance = active_seller.wallet_balance.as_ref() + trade.total_amount;
            active_seller.wallet_balance = Set(new_seller_balance);
            active_seller.updated_at = Set(chrono::Utc::now().naive_utc());
            if let Err(e) = active_seller.update(&txn).await {
                log::error!("Failed to update seller balance: {}", e);
                let _ = txn.rollback().await;
                return Err(actix_web::error::ErrorInternalServerError(
                    "Failed to update balance",
                ));
            }

            // Track balance changes for response
            if trade.buyer_id == user_id_int {
                current_balance -= trade.total_amount;
            } else if trade.seller_id == user_id_int {
                current_balance += trade.total_amount;
            }

            // Update order statuses in database
            let buy_order = Order {
                id: trade.buy_order_id.clone(),
                user_id: trade.buyer_id,
                event_id: trade.event_id,
                option_id: trade.option_id,
                side: OrderSide::Buy,
                order_type: OrderType::Limit,
                time_in_force: TimeInForce::GTC,
                price: trade.price,
                quantity: 0,
                filled_quantity: trade.quantity,
                status: OrderStatus::Filled,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let _ = db_persistence.update_order(&buy_order).await;

            let sell_order = Order {
                id: trade.sell_order_id.clone(),
                user_id: trade.seller_id,
                event_id: trade.event_id,
                option_id: trade.option_id,
                side: OrderSide::Sell,
                order_type: OrderType::Limit,
                time_in_force: TimeInForce::GTC,
                price: trade.price,
                quantity: 0,
                filled_quantity: trade.quantity,
                status: OrderStatus::Filled,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let _ = db_persistence.update_order(&sell_order).await;
        }

        txn.commit().await.map_err(|e| {
            log::error!("Failed to commit transaction: {}", e);
            actix_web::error::ErrorInternalServerError("Transaction error")
        })?;

        current_balance
    } else {
        user.wallet_balance
    };

    // Convert trades to response format
    let trade_responses: Vec<TradeResponse> = trades
        .into_iter()
        .map(|t| TradeResponse {
            id: t.id,
            event_id: t.event_id,
            option_id: t.option_id,
            buyer_id: t.buyer_id,
            seller_id: t.seller_id,
            price: t.price,
            quantity: t.quantity,
            total_amount: t.total_amount,
            timestamp: t.timestamp,
        })
        .collect();

    // Update event option price immediately based on order book (event-driven)
    let db_clone = db.clone();
    let redis_pool_clone = redis_pool.clone();
    let ws_server_clone = ws_server.clone();
    let event_id = req.event_id;
    let option_id = req.option_id;
    tokio::spawn(async move {
        crate::order_book::price_updater::update_option_price_immediately(
            db_clone,
            redis_pool_clone,
            ws_server_clone,
            event_id,
            option_id,
        )
        .await;
    });

    // Invalidate caches
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let event_cache_key = create_cache_key(cache_keys::EVENT_PREFIX, &req.event_id.to_string());
    let option_cache_key = format!("event_option:{}", req.option_id);
    let order_book_cache_key = format!("order_book:{}:{}", req.event_id, req.option_id);

    if let Err(e) = cache_service.delete(&event_cache_key).await {
        log::warn!("Failed to invalidate event cache: {}", e);
    }
    if let Err(e) = cache_service.delete(&option_cache_key).await {
        log::warn!("Failed to invalidate option cache: {}", e);
    }
    if let Err(e) = cache_service.delete(&order_book_cache_key).await {
        log::warn!("Failed to invalidate order book cache: {}", e);
    }

    Ok(HttpResponse::Ok().json(PlaceOrderResponse {
        success: true,
        order_id,
        trades: trade_responses,
        wallet_balance: updated_balance,
    }))
}

pub async fn cancel_order(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    req: web::Json<CancelOrderRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    let db_persistence = DbPersistence::new(db.get_ref().clone());
    let redis_persistence = RedisOrderBookPersistence::new(redis_pool.get_ref().clone());

    // Load the order to find which order book it belongs to
    let order_to_cancel = redis_persistence
        .load_order(&req.order_id)
        .await
        .map_err(|e| {
            log::error!("Failed to load order: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to load order")
        })?;

    let order_to_cancel = match order_to_cancel {
        Some(order) => {
            // Verify the order belongs to the user
            if order.user_id != user_id_int {
                return Ok(HttpResponse::Forbidden().json(json!({
                    "success": false,
                    "message": "You can only cancel your own orders"
                })));
            }
            order
        }
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "success": false,
                "message": "Order not found"
            })));
        }
    };

    let event_id = order_to_cancel.event_id;
    let option_id = order_to_cancel.option_id;

    // Load the order book
    let mut order_book = redis_persistence
        .get_or_create_order_book(event_id, option_id)
        .await
        .map_err(|e| {
            log::error!("Failed to get order book from Redis: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to get order book")
        })?;

    // Cancel the order
    let cancelled_order = order_book.cancel_order(&req.order_id).map_err(|e| {
        log::error!("Failed to cancel order: {}", e);
        actix_web::error::ErrorInternalServerError(e)
    })?;

    // Save updated order book back to Redis
    if let Err(e) = redis_persistence
        .save_full_order_book(event_id, option_id, &order_book)
        .await
    {
        log::error!("Failed to save updated order book to Redis: {}", e);
    }

    // Update order status in database
    if let Err(e) = db_persistence.update_order(&cancelled_order).await {
        log::error!("Failed to update order status in database: {}", e);
    }

    // Update order status in Redis
    if let Err(e) = redis_persistence
        .update_order_status(
            &cancelled_order.id,
            OrderStatus::Cancelled,
            cancelled_order.filled_quantity,
        )
        .await
    {
        log::error!("Failed to update order status in Redis: {}", e);
    }

    // Update event option price immediately based on order book (event-driven)
    let db_clone = db.clone();
    let redis_pool_clone = redis_pool.clone();
    let ws_server_clone = ws_server.clone();
    tokio::spawn(async move {
        crate::order_book::price_updater::update_option_price_immediately(
            db_clone,
            redis_pool_clone,
            ws_server_clone,
            event_id,
            option_id,
        )
        .await;
    });

    // Invalidate caches
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let order_book_cache_key = format!("order_book:{}:{}", event_id, option_id);

    if let Err(e) = cache_service.delete(&order_book_cache_key).await {
        log::warn!("Failed to invalidate order book cache: {}", e);
    }

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "message": "Order cancelled successfully",
        "order": OrderResponse {
            id: cancelled_order.id,
            user_id: cancelled_order.user_id,
            event_id: cancelled_order.event_id,
            option_id: cancelled_order.option_id,
            side: cancelled_order.side,
            order_type: cancelled_order.order_type,
            time_in_force: cancelled_order.time_in_force,
            price: cancelled_order.price,
            quantity: cancelled_order.quantity,
            filled_quantity: cancelled_order.filled_quantity,
            status: cancelled_order.status,
            created_at: cancelled_order.created_at,
            updated_at: cancelled_order.updated_at,
        }
    })))
}

pub async fn get_order_book(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    path: web::Path<(i32, i32)>,
) -> Result<HttpResponse, Error> {
    let (event_id, option_id) = path.into_inner();
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let cache_key = format!("order_book:{}:{}", event_id, option_id);

    // Try to get from cache first
    if let Ok(Some(cached_response)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(cached_response));
    }

    // Verify the event and option exist
    let event = events::Entity::find_by_id(event_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if event.is_none() {
        return Ok(HttpResponse::NotFound().json(json!({
            "success": false,
            "message": "Event not found"
        })));
    }

    let option = event_options::Entity::find_by_id(option_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if option.is_none() || option.as_ref().unwrap().event_id != event_id {
        return Ok(HttpResponse::NotFound().json(json!({
            "success": false,
            "message": "Option not found"
        })));
    }

    // Get order book from Redis
    let redis_persistence = RedisOrderBookPersistence::new(redis_pool.get_ref().clone());
    let order_book = redis_persistence
        .get_or_create_order_book(event_id, option_id)
        .await
        .map_err(|e| {
            log::error!("Failed to get order book from Redis: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to get order book")
        })?;

    let snapshot = order_book.get_snapshot();
    let predicted_price = order_book.get_predicted_price();

    let response = OrderBookResponse {
        event_id: snapshot.event_id,
        option_id: snapshot.option_id,
        bids: snapshot.bids.into_iter().map(|l| l.into()).collect(),
        asks: snapshot.asks.into_iter().map(|l| l.into()).collect(),
        last_trade_price: snapshot.last_trade_price,
        mid_price: snapshot.mid_price,
        spread: snapshot.spread,
        predicted_price,
    };

    let response_json = json!({
        "success": true,
        "order_book": response
    });

    // Cache the response for 30 seconds
    if let Err(e) = cache_service.set(&cache_key, &response_json, 30).await {
        log::warn!("Failed to cache order book: {}", e);
    }

    Ok(HttpResponse::Ok().json(response_json))
}

pub async fn get_market_depth(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    path: web::Path<(i32, i32)>,
) -> Result<HttpResponse, Error> {
    let (event_id, option_id) = path.into_inner();

    // Verify the event and option exist
    let event = events::Entity::find_by_id(event_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if event.is_none() {
        return Ok(HttpResponse::NotFound().json(json!({
            "success": false,
            "message": "Event not found"
        })));
    }

    let option = event_options::Entity::find_by_id(option_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if option.is_none() || option.as_ref().unwrap().event_id != event_id {
        return Ok(HttpResponse::NotFound().json(json!({
            "success": false,
            "message": "Option not found"
        })));
    }

    // Get order book from Redis
    let redis_persistence = RedisOrderBookPersistence::new(redis_pool.get_ref().clone());
    let order_book = redis_persistence
        .get_or_create_order_book(event_id, option_id)
        .await
        .map_err(|e| {
            log::error!("Failed to get order book from Redis: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to get order book")
        })?;

    let depth = order_book.get_market_depth(20); // Get top 20 levels

    let total_bid_volume: i32 = depth.iter().map(|d| d.buy_quantity).sum();
    let total_ask_volume: i32 = depth.iter().map(|d| d.sell_quantity).sum();

    let response = MarketDepthResponse {
        event_id,
        option_id,
        depth,
        total_bid_volume,
        total_ask_volume,
    };

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "market_depth": response
    })))
}

pub async fn get_user_orders(
    db: web::Data<DatabaseConnection>,
    _redis_pool: web::Data<Pool>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    let db_persistence = DbPersistence::new(db.get_ref().clone());

    // Get user's orders from database
    let orders = db_persistence
        .get_user_orders(user_id_int, None, 100)
        .await
        .map_err(|e| {
            log::error!("Failed to get user orders: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve orders")
        })?;

    let order_responses: Vec<OrderResponse> = orders
        .into_iter()
        .map(|order| OrderResponse {
            id: order.id,
            user_id: order.user_id,
            event_id: order.event_id,
            option_id: order.option_id,
            side: order.side,
            order_type: order.order_type,
            time_in_force: order.time_in_force,
            price: order.price,
            quantity: order.quantity,
            filled_quantity: order.filled_quantity,
            status: order.status,
            created_at: order.created_at,
            updated_at: order.updated_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "orders": order_responses
    })))
}

pub async fn get_trade_history(
    db: web::Data<DatabaseConnection>,
    _redis_pool: web::Data<Pool>,
    path: web::Path<(i32, i32)>,
) -> Result<HttpResponse, Error> {
    let (event_id, option_id) = path.into_inner();

    // Verify the event and option exist
    let event = events::Entity::find_by_id(event_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if event.is_none() {
        return Ok(HttpResponse::NotFound().json(json!({
            "success": false,
            "message": "Event not found"
        })));
    }

    let option = event_options::Entity::find_by_id(option_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if option.is_none() || option.as_ref().unwrap().event_id != event_id {
        return Ok(HttpResponse::NotFound().json(json!({
            "success": false,
            "message": "Option not found"
        })));
    }

    let db_persistence = DbPersistence::new(db.get_ref().clone());

    // Get trades from database
    let trades = db_persistence
        .get_event_option_trades(event_id, option_id, 100)
        .await
        .map_err(|e| {
            log::error!("Failed to get trade history: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve trades")
        })?;

    let trade_responses: Vec<TradeResponse> = trades
        .into_iter()
        .map(|trade| TradeResponse {
            id: trade.id,
            event_id: trade.event_id,
            option_id: trade.option_id,
            buyer_id: trade.buyer_id,
            seller_id: trade.seller_id,
            price: trade.price,
            quantity: trade.quantity,
            total_amount: trade.total_amount,
            timestamp: trade.timestamp,
        })
        .collect();

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "trades": trade_responses
    })))
}
