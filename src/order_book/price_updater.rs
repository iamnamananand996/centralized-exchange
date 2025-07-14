use super::redis_persistence::RedisOrderBookPersistence;
use crate::utils::cache::{cache_keys, create_cache_key, CacheService};
use crate::websocket::server::WebSocketServer;
use actix::Addr;
use actix_web::web;
use deadpool_redis::Pool;
use entity::event_options;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};

/// Update event option prices based on order book data
pub async fn update_option_prices(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
) {
    let redis_persistence = RedisOrderBookPersistence::new(redis_pool.get_ref().clone());
    
    // Get all active event options from database
    let options = match event_options::Entity::find()
        .all(db.get_ref())
        .await
    {
        Ok(opts) => opts,
        Err(e) => {
            log::error!("Failed to get event options: {}", e);
            return;
        }
    };

    let mut price_updates = Vec::new();

    // Check each option's order book for price updates
    for option in options {
        match redis_persistence
            .load_full_order_book(option.event_id, option.id)
            .await
        {
            Ok(Some(order_book)) => {
                if let Some(predicted_price) = order_book.get_predicted_price() {
                    price_updates.push((option.event_id, option.id, predicted_price));
                }
            }
            Ok(None) => {
                // No order book exists for this option yet
                continue;
            }
            Err(e) => {
                log::error!(
                    "Failed to load order book for option {}: {}",
                    option.id,
                    e
                );
                continue;
            }
        }
    }

    let cache_service = CacheService::new(redis_pool.get_ref().clone());

    for (event_id, option_id, predicted_price) in price_updates {
            // Fetch the current option from database
            match event_options::Entity::find_by_id(option_id)
                .one(db.get_ref())
                .await
            {
                Ok(Some(option)) => {
                    // Only update if price has changed significantly (more than 0.5%)
                    let price_change_ratio = ((predicted_price - option.current_price)
                        / option.current_price)
                        .abs();

                    if price_change_ratio > sea_orm::prelude::Decimal::new(5, 3) {
                        // 0.005 = 0.5%
                        let mut active_option: event_options::ActiveModel = option.into();
                        active_option.current_price = Set(predicted_price);

                        match active_option.update(db.get_ref()).await {
                            Ok(_updated_option) => {
                                log::info!(
                                    "Updated option {} price to {}",
                                    option_id,
                                    predicted_price
                                );

                                // Invalidate caches
                                let event_cache_key =
                                    create_cache_key(cache_keys::EVENT_PREFIX, &event_id.to_string());
                                let option_cache_key = format!("event_option:{}", option_id);

                                if let Err(e) = cache_service.delete(&event_cache_key).await {
                                    log::warn!("Failed to invalidate event cache: {}", e);
                                }
                                if let Err(e) = cache_service.delete(&option_cache_key).await {
                                    log::warn!("Failed to invalidate option cache: {}", e);
                                }

                                // Broadcast updates
                                let handlers = crate::websocket::handlers::WebSocketHandlers::new(
                                    db.clone(),
                                    ws_server.get_ref().clone(),
                                );

                                let event_id_for_broadcast = event_id;
                                tokio::spawn(async move {
                                    handlers.fetch_and_broadcast_event(event_id_for_broadcast).await;
                                });

                                // Broadcast events update
                                ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);
                            }
                            Err(e) => {
                                log::error!("Failed to update option price: {}", e);
                            }
                        }
                    }
                }
                Ok(None) => {
                    log::warn!("Option {} not found in database", option_id);
                }
                Err(e) => {
                    log::error!("Database error when fetching option: {}", e);
                }
            }
    }
}

/// Start a background task to periodically update prices
pub fn start_price_updater(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30)); // Update every 30 seconds

        loop {
            interval.tick().await;
            update_option_prices(
                db.clone(),
                redis_pool.clone(),
                ws_server.clone(),
            )
            .await;
        }
    });
} 