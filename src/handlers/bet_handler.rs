use crate::types::bet::{
    ActivePosition, BetResponse, BetsSummary, EventSummary, MyBetResponse, MyBetsQuery,
    OptionSummary, PlaceBetRequest, PortfolioResponse, PositionDetail,
};
use crate::utils::cache::{cache_keys, create_cache_key, CacheService};
use crate::utils::pagination::{PaginatedResponse, PaginationInfo};
use crate::websocket::server::WebSocketServer;
use actix::Addr;
use actix_web::{web, Error, HttpResponse, Result};
use deadpool_redis::Pool;
use entity::{bets, event_options, events, users};
use sea_orm::{
    prelude::Decimal, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait, Set,
};
use serde_json::json;

pub async fn place_bet(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    req: web::Json<PlaceBetRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

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
            "message": "Event is not active for betting"
        })));
    }

    // Check if event has ended
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

    // Calculate total amount
    let total_amount = req.price_per_share * Decimal::from(req.quantity);

    // Validate betting amount limits
    if total_amount < event.min_bet_amount {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": format!("Bet amount must be at least {}", event.min_bet_amount)
        })));
    }

    if total_amount > event.max_bet_amount {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": format!("Bet amount cannot exceed {}", event.max_bet_amount)
        })));
    }

    // Check if user has sufficient balance
    if user.wallet_balance < total_amount {
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": "Insufficient balance"
        })));
    }

    // Create the bet
    let new_bet = bets::ActiveModel {
        user_id: Set(user_id_int),
        event_id: Set(req.event_id),
        option_id: Set(req.option_id),
        quantity: Set(req.quantity),
        price_per_share: Set(req.price_per_share),
        total_amount: Set(total_amount),
        status: Set("active".to_string()),
        placed_at: Set(chrono::Utc::now().naive_utc()),
        settled_at: Set(None),
        payout_amount: Set(Decimal::new(0, 2)),
        ..Default::default()
    };

    let bet = new_bet.insert(db.get_ref()).await.map_err(|e| {
        log::error!("Bet creation error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to place bet")
    })?;

    // Update user balance
    let user_wallet_balance = user.wallet_balance;
    let mut active_user: users::ActiveModel = user.into();
    active_user.wallet_balance = Set(user_wallet_balance - total_amount);
    let updated_user = active_user.update(db.get_ref()).await.map_err(|e| {
        log::error!("User balance update error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to update user balance")
    })?;

    // Update event total volume
    let event_total_volume = event.total_volume;
    let mut active_event: events::ActiveModel = event.into();
    active_event.total_volume = Set(event_total_volume + total_amount);
    active_event.update(db.get_ref()).await.map_err(|e| {
        log::error!("Event volume update error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to update event volume")
    })?;

    // Update option total backing
    let option_total_backing = option.total_backing;
    let mut active_option: event_options::ActiveModel = option.into();
    active_option.total_backing = Set(option_total_backing + total_amount);
    active_option.update(db.get_ref()).await.map_err(|e| {
        log::error!("Option backing update error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to update option backing")
    })?;

    let bet_response = BetResponse::from(bet);

    // Invalidate relevant caches
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let user_cache_key = create_cache_key(cache_keys::USER_PREFIX, &user_id_int.to_string());
    let event_cache_key = create_cache_key(cache_keys::EVENT_PREFIX, &req.event_id.to_string());
    let option_cache_key = format!("event_option:{}", req.option_id);
    let portfolio_cache_key = format!("portfolio:{}", user_id_int);

    // Delete relevant caches
    if let Err(e) = cache_service.delete(&user_cache_key).await {
        log::warn!("Failed to invalidate user cache: {}", e);
    }
    if let Err(e) = cache_service.delete(&event_cache_key).await {
        log::warn!("Failed to invalidate event cache: {}", e);
    }
    if let Err(e) = cache_service.delete(&option_cache_key).await {
        log::warn!("Failed to invalidate option cache: {}", e);
    }
    if let Err(e) = cache_service.delete(&portfolio_cache_key).await {
        log::warn!("Failed to invalidate portfolio cache: {}", e);
    }

    // Notify subscribers via WebSocket
    let user_id_int = user_id_str.parse::<i32>().unwrap_or(0);

    // Send personalized updates to WebSocket subscribers
    ws_server.do_send(crate::websocket::server::BroadcastMyBetsUpdate {
        user_id: user_id_int,
    });

    // Broadcast portfolio update (no pagination, so use existing method)
    let handlers =
        crate::websocket::handlers::WebSocketHandlers::new(db.clone(), ws_server.get_ref().clone());

    // Clone event_id for the async block
    let event_id_for_broadcast = req.event_id;

    tokio::spawn(async move {
        // Update portfolio data for the user
        handlers.fetch_and_broadcast_portfolio(user_id_int).await;

        // Broadcast to specific event channel (event:24) since bet updated total_volume
        handlers
            .fetch_and_broadcast_event(event_id_for_broadcast)
            .await;
    });

    // Broadcast events update to all subscribers of the events channel
    ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "bet": bet_response,
        "wallet_balance": updated_user.wallet_balance
    })))
}

pub async fn get_my_bets(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    query: web::Query<MyBetsQuery>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    let cache_service = CacheService::new(redis_pool.get_ref().clone());

    // Create cache key based on user ID, status filter, and pagination
    let cache_key = format!(
        "bets:{}:{}:{}:{}",
        user_id_int,
        query.status.as_deref().unwrap_or("all"),
        query.pagination.get_page(),
        query.pagination.get_limit()
    );

    // Try to get from cache first (shorter TTL for bet data)
    if let Ok(Some(cached_response)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(cached_response));
    }

    let mut bets_query = bets::Entity::find()
        .filter(bets::Column::UserId.eq(user_id_int))
        .find_also_related(events::Entity)
        .find_also_related(event_options::Entity);

    // Apply status filter
    if let Some(status) = &query.status {
        bets_query = bets_query.filter(bets::Column::Status.eq(status));
    }

    // Apply pagination
    let page = query.pagination.get_page();
    let limit = query.pagination.get_limit();
    let offset = query.pagination.get_offset();

    // Get total count
    let total_count = bets::Entity::find()
        .filter(bets::Column::UserId.eq(user_id_int))
        .apply_if(query.status.as_ref(), |query, status| {
            query.filter(bets::Column::Status.eq(status))
        })
        .count(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    // Get bets with related data
    let bets_with_relations = bets_query
        .order_by_desc(bets::Column::PlacedAt)
        .limit(limit)
        .offset(offset)
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let mut my_bets: Vec<MyBetResponse> = Vec::new();
    let mut total_invested = Decimal::new(0, 2);
    let mut current_value = Decimal::new(0, 2);

    for (bet, event_opt, option_opt) in bets_with_relations {
        let event = event_opt.unwrap_or_else(|| events::Model {
            id: 0,
            title: "Unknown Event".to_string(),
            status: "unknown".to_string(),
            // ... other fields with default values
            description: "".to_string(),
            category: "".to_string(),
            end_time: chrono::NaiveDateTime::default(),
            min_bet_amount: Decimal::new(0, 2),
            max_bet_amount: Decimal::new(0, 2),
            total_volume: Decimal::new(0, 2),
            image_url: "".to_string(),
            created_by: 0,
            resolved_by: 0,
            winning_option_id: 0,
            resolution_note: "".to_string(),
            resolved_at: chrono::NaiveDateTime::default(),
            created_at: chrono::NaiveDateTime::default(),
            updated_at: chrono::NaiveDateTime::default(),
        });

        let option = option_opt.unwrap_or_else(|| event_options::Model {
            id: 0,
            event_id: 0,
            option_text: "Unknown Option".to_string(),
            current_price: Decimal::new(0, 2),
            total_backing: Decimal::new(0, 2),
            is_winning_option: None,
        });

        let bet_current_value = option.current_price * Decimal::from(bet.quantity);
        let pnl = bet_current_value - bet.total_amount;

        total_invested += bet.total_amount;
        current_value += bet_current_value;

        my_bets.push(MyBetResponse {
            id: bet.id,
            event: EventSummary {
                id: event.id,
                title: event.title,
                status: event.status,
            },
            option: OptionSummary {
                id: option.id,
                option_text: option.option_text,
                current_price: option.current_price,
            },
            quantity: bet.quantity,
            price_per_share: bet.price_per_share,
            total_amount: bet.total_amount,
            current_value: bet_current_value,
            pnl,
            status: bet.status,
            placed_at: bet.placed_at.and_utc(),
        });
    }

    let total_pnl = current_value - total_invested;
    let summary = BetsSummary {
        total_invested,
        current_value,
        total_pnl,
    };

    let pagination_info = PaginationInfo::new(page, total_count, limit);
    let response = PaginatedResponse::new(my_bets, pagination_info);

    let response_json = json!({
        "success": true,
        "bets": response.data,
        "summary": summary,
        "pagination": response.pagination,
    });

    // Cache the response for 2 minutes (shorter TTL for bet data)
    if let Err(e) = cache_service.set(&cache_key, &response_json, 120).await {
        log::warn!("Failed to cache my bets: {}", e);
    }

    Ok(HttpResponse::Ok().json(response_json))
}

pub async fn get_portfolio(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let cache_key = format!("portfolio:{}", user_id_int);

    // Try to get from cache first (shorter TTL for portfolio data)
    if let Ok(Some(cached_portfolio)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(cached_portfolio));
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

    // Get all active bets with related data
    let active_bets = bets::Entity::find()
        .filter(bets::Column::UserId.eq(user_id_int))
        .filter(bets::Column::Status.eq("active"))
        .find_also_related(events::Entity)
        .find_also_related(event_options::Entity)
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let mut total_invested = Decimal::new(0, 2);
    let mut current_value = Decimal::new(0, 2);
    let mut positions_map: std::collections::HashMap<i32, ActivePosition> =
        std::collections::HashMap::new();

    for (bet, event_opt, option_opt) in active_bets {
        let event = event_opt.unwrap_or_else(|| events::Model {
            id: 0,
            title: "Unknown Event".to_string(),
            status: "unknown".to_string(),
            description: "".to_string(),
            category: "".to_string(),
            end_time: chrono::NaiveDateTime::default(),
            min_bet_amount: Decimal::new(0, 2),
            max_bet_amount: Decimal::new(0, 2),
            total_volume: Decimal::new(0, 2),
            image_url: "".to_string(),
            created_by: 0,
            resolved_by: 0,
            winning_option_id: 0,
            resolution_note: "".to_string(),
            resolved_at: chrono::NaiveDateTime::default(),
            created_at: chrono::NaiveDateTime::default(),
            updated_at: chrono::NaiveDateTime::default(),
        });

        let option = option_opt.unwrap_or_else(|| event_options::Model {
            id: 0,
            event_id: 0,
            option_text: "Unknown Option".to_string(),
            current_price: Decimal::new(0, 2),
            total_backing: Decimal::new(0, 2),
            is_winning_option: None,
        });

        let bet_current_value = option.current_price * Decimal::from(bet.quantity);
        total_invested += bet.total_amount;
        current_value += bet_current_value;

        let position = positions_map
            .entry(event.id)
            .or_insert_with(|| ActivePosition {
                event_id: event.id,
                event_title: event.title.clone(),
                invested: Decimal::new(0, 2),
                current_value: Decimal::new(0, 2),
                pnl: Decimal::new(0, 2),
                positions: Vec::new(),
            });

        position.invested += bet.total_amount;
        position.current_value += bet_current_value;
        position.pnl = position.current_value - position.invested;

        position.positions.push(PositionDetail {
            option_text: option.option_text,
            quantity: bet.quantity,
            avg_price: bet.price_per_share,
            current_price: option.current_price,
        });
    }

    let total_pnl = current_value - total_invested;
    let active_positions: Vec<ActivePosition> = positions_map.into_values().collect();

    let portfolio = PortfolioResponse {
        total_invested,
        current_value,
        total_pnl,
        wallet_balance: user.wallet_balance,
        active_positions,
    };

    let response_json = json!({
        "success": true,
        "portfolio": portfolio
    });

    // Cache the response for 2 minutes (shorter TTL for portfolio data)
    if let Err(e) = cache_service.set(&cache_key, &response_json, 120).await {
        log::warn!("Failed to cache portfolio: {}", e);
    }

    Ok(HttpResponse::Ok().json(response_json))
}
