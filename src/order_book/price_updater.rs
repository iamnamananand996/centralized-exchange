use super::redis_persistence::RedisOrderBookPersistence;
use crate::constants::config;
use crate::utils::cache::{cache_keys, create_cache_key, CacheService};
use crate::websocket::server::WebSocketServer;
use actix::Addr;
use actix_web::web;
use deadpool_redis::Pool;
use entity::{event_options, events};
use sea_orm::prelude::Decimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, JoinType, QueryFilter,
    QuerySelect, RelationTrait, Set,
};

/// Update event option prices based on order book data for active events only
pub async fn update_option_prices(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
) {
    let redis_persistence = RedisOrderBookPersistence::new(redis_pool.get_ref().clone());

    // Get only event options from active events to reduce database load
    let options = match event_options::Entity::find()
        .join(JoinType::InnerJoin, event_options::Relation::Events.def())
        .filter(events::Column::Status.eq("active"))
        .all(db.get_ref())
        .await
    {
        Ok(opts) => opts,
        Err(e) => {
            log::error!("Failed to get active event options: {}", e);
            return;
        }
    };

    log::info!(
        "Checking {} active event options for price updates",
        options.len()
    );

    let mut price_updates = Vec::new();

    // Check each option's order book for price updates
    for option in options {
        // Only check options that have order books (skip empty ones)
        match redis_persistence
            .load_full_order_book(option.event_id, option.id)
            .await
        {
            Ok(Some(order_book)) => {
                if let Some(predicted_price) = order_book.get_predicted_price() {
                    // Only update if price has changed significantly (more than 0.5%)
                    let price_change_ratio =
                        ((predicted_price - option.current_price) / option.current_price).abs();

                    if price_change_ratio > Decimal::new(5, 3) {
                        // 0.005 = 0.5%
                        price_updates.push((
                            option.event_id,
                            option.id,
                            predicted_price,
                            option.current_price,
                        ));
                    }
                }
            }
            Ok(None) => {
                // No order book exists for this option - skip
                continue;
            }
            Err(e) => {
                log::error!("Failed to load order book for option {}: {}", option.id, e);
                continue;
            }
        }
    }

    if price_updates.is_empty() {
        log::debug!("No significant price changes detected");
        return;
    }

    log::info!("Updating {} option prices", price_updates.len());

    // Batch update prices
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let mut events_to_broadcast = std::collections::HashSet::new();

    for (event_id, option_id, new_price, old_price) in price_updates {
        if let Err(e) = update_single_option_price(
            &db,
            &cache_service,
            event_id,
            option_id,
            new_price,
            old_price,
        )
        .await
        {
            log::error!("Failed to update price for option {}: {}", option_id, e);
            continue;
        }

        events_to_broadcast.insert(event_id);
    }

    // Broadcast updates for all affected events
    if !events_to_broadcast.is_empty() {
        let _handlers = crate::websocket::handlers::WebSocketHandlers::new(
            db.clone(),
            ws_server.get_ref().clone(),
        );

        for event_id in events_to_broadcast {
            let handlers_clone = crate::websocket::handlers::WebSocketHandlers::new(
                db.clone(),
                ws_server.get_ref().clone(),
            );
            tokio::spawn(async move {
                handlers_clone.fetch_and_broadcast_event(event_id).await;
            });
        }

        // Broadcast general events update
        ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);
    }
}

/// Update a single option's price (can be called from order handlers for event-driven updates)
pub async fn update_single_option_price(
    db: &web::Data<DatabaseConnection>,
    cache_service: &CacheService,
    event_id: i32,
    option_id: i32,
    new_price: Decimal,
    old_price: Decimal,
) -> Result<(), String> {
    // Update the database
    let option = event_options::Entity::find_by_id(option_id)
        .one(db.get_ref())
        .await
        .map_err(|e| format!("Failed to find option: {}", e))?
        .ok_or("Option not found")?;

    let mut active_option: event_options::ActiveModel = option.into();
    active_option.current_price = Set(new_price);

    active_option
        .update(db.get_ref())
        .await
        .map_err(|e| format!("Failed to update option price: {}", e))?;

    log::info!(
        "Updated option {} price from {} to {} (change: {:.2}%)",
        option_id,
        old_price,
        new_price,
        ((new_price - old_price) / old_price * Decimal::new(100, 0))
    );

    // Invalidate relevant caches
    let event_cache_key = create_cache_key(cache_keys::EVENT_PREFIX, &event_id.to_string());
    let option_cache_key = format!("event_option:{}", option_id);

    if let Err(e) = cache_service.delete(&event_cache_key).await {
        log::warn!("Failed to invalidate event cache: {}", e);
    }
    if let Err(e) = cache_service.delete(&option_cache_key).await {
        log::warn!("Failed to invalidate option cache: {}", e);
    }

    Ok(())
}

/// Update price for a specific option immediately (event-driven)
pub async fn update_option_price_immediately(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    event_id: i32,
    option_id: i32,
) {
    let redis_persistence = RedisOrderBookPersistence::new(redis_pool.get_ref().clone());
    let cache_service = CacheService::new(redis_pool.get_ref().clone());

    // Get current price from database
    let current_price = match event_options::Entity::find_by_id(option_id)
        .one(db.get_ref())
        .await
    {
        Ok(Some(option)) => option.current_price,
        Ok(None) => {
            log::warn!("Option {} not found for immediate price update", option_id);
            return;
        }
        Err(e) => {
            log::error!("Failed to get option for immediate price update: {}", e);
            return;
        }
    };

    // Get predicted price from order book
    let predicted_price = match redis_persistence
        .load_full_order_book(event_id, option_id)
        .await
    {
        Ok(Some(order_book)) => {
            if let Some(price) = order_book.get_predicted_price() {
                price
            } else {
                log::debug!("No predicted price available for option {}", option_id);
                return;
            }
        }
        Ok(None) => {
            log::debug!("No order book found for option {}", option_id);
            return;
        }
        Err(e) => {
            log::error!(
                "Failed to load order book for immediate price update: {}",
                e
            );
            return;
        }
    };

    // Check if price change is significant
    let price_change_ratio = ((predicted_price - current_price) / current_price).abs();
    if price_change_ratio <= Decimal::new(5, 3) {
        // Less than 0.5% change - not significant enough
        return;
    }

    // Update the price
    if let Err(e) = update_single_option_price(
        &db,
        &cache_service,
        event_id,
        option_id,
        predicted_price,
        current_price,
    )
    .await
    {
        log::error!("Failed to update option price immediately: {}", e);
        return;
    }

    // Broadcast the update
    let handlers =
        crate::websocket::handlers::WebSocketHandlers::new(db.clone(), ws_server.get_ref().clone());

    tokio::spawn(async move {
        handlers.fetch_and_broadcast_event(event_id).await;
    });

    ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);
}

/// Start a background task to periodically update prices for active events
pub fn start_price_updater(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
) {
    let interval_seconds = config::get_price_update_interval_seconds();

    log::info!(
        "Starting price updater with {}-second interval (checking active events only)",
        interval_seconds
    );

    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(interval_seconds));

        loop {
            interval.tick().await;

            let start_time = std::time::Instant::now();
            update_option_prices(db.clone(), redis_pool.clone(), ws_server.clone()).await;

            let elapsed = start_time.elapsed();
            log::debug!("Price update cycle completed in {:?}", elapsed);
        }
    });
}
