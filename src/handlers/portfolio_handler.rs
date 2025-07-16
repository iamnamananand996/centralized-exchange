use crate::order_book::position_tracker::PositionTracker;
use crate::order_book::types::UserPosition;
use crate::utils::cache::CacheService;
use actix_web::{web, Error, HttpResponse, Result};
use deadpool_redis::Pool;
use entity::{event_options, events, users};
use sea_orm::prelude::Decimal;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct PortfolioResponse {
    pub total_invested: Decimal,
    pub current_value: Decimal,
    pub total_pnl: Decimal,
    pub wallet_balance: Decimal,
    pub active_positions: Vec<EventPositionGroup>,
}

#[derive(Serialize)]
pub struct EventPositionGroup {
    pub event_id: i32,
    pub event_title: String,
    pub event_status: String,
    pub invested: Decimal,
    pub current_value: Decimal,
    pub pnl: Decimal,
    pub positions: Vec<PositionDetail>,
}

#[derive(Serialize)]
pub struct PositionDetail {
    pub option_id: i32,
    pub option_text: String,
    pub quantity: i32,
    pub avg_price: Decimal,
    pub current_price: Decimal,
    pub position_value: Decimal,
}

#[derive(Serialize)]
pub struct PortfolioSummary {
    pub total_positions: usize,
    pub active_events: usize,
    pub total_invested: Decimal,
    pub current_value: Decimal,
    pub total_pnl: Decimal,
    pub pnl_percentage: Decimal,
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

    // Try to get from cache first
    if let Ok(Some(cached_portfolio)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(cached_portfolio));
    }

    // Get user data
    let user = users::Entity::find_by_id(user_id_int)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Failed to get user: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve user")
        })?
        .ok_or_else(|| actix_web::error::ErrorNotFound("User not found"))?;

    let position_tracker = PositionTracker::new(db.get_ref().clone());

    // Get all user positions
    let positions = position_tracker
        .get_user_positions(user_id_int)
        .await
        .map_err(|e| {
            log::error!("Failed to get user positions: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve positions")
        })?;

    // Group positions by event
    let grouped_positions = position_tracker
        .get_portfolio_positions(user_id_int)
        .await
        .map_err(|e| {
            log::error!("Failed to get portfolio positions: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve portfolio")
        })?;

    let mut total_invested = Decimal::new(0, 2);
    let mut current_value = Decimal::new(0, 2);
    let mut active_positions = Vec::new();

    // Process each event group
    for (event_id, event_positions) in grouped_positions {
        // Get event details
        let event = events::Entity::find_by_id(event_id)
            .one(db.get_ref())
            .await
            .map_err(|e| {
                log::error!("Failed to get event: {}", e);
                actix_web::error::ErrorInternalServerError("Failed to retrieve event")
            })?;

        let event = match event {
            Some(e) => e,
            None => continue, // Skip if event not found
        };

        let mut event_invested = Decimal::new(0, 2);
        let mut event_current_value = Decimal::new(0, 2);
        let mut position_details = Vec::new();

        // Process each position in the event
        for position in event_positions {
            // Get option details
            let option = event_options::Entity::find_by_id(position.option_id)
                .one(db.get_ref())
                .await
                .map_err(|e| {
                    log::error!("Failed to get option: {}", e);
                    actix_web::error::ErrorInternalServerError("Failed to retrieve option")
                })?;

            let option = match option {
                Some(o) => o,
                None => continue, // Skip if option not found
            };

            let position_cost = position.average_price * Decimal::from(position.quantity);
            let position_value = option.current_price * Decimal::from(position.quantity);

            event_invested += position_cost;
            event_current_value += position_value;

            position_details.push(PositionDetail {
                option_id: position.option_id,
                option_text: option.option_text,
                quantity: position.quantity,
                avg_price: position.average_price,
                current_price: option.current_price,
                position_value,
            });
        }

        let event_pnl = event_current_value - event_invested;

        total_invested += event_invested;
        current_value += event_current_value;

        active_positions.push(EventPositionGroup {
            event_id: event.id,
            event_title: event.title,
            event_status: event.status,
            invested: event_invested,
            current_value: event_current_value,
            pnl: event_pnl,
            positions: position_details,
        });
    }

    let total_pnl = current_value - total_invested;

    let portfolio = PortfolioResponse {
        total_invested,
        current_value,
        total_pnl,
        wallet_balance: user.wallet_balance,
        active_positions,
    };

    let response = json!({
        "success": true,
        "portfolio": portfolio
    });

    // Cache the response for 2 minutes
    if let Err(e) = cache_service.set(&cache_key, &response, 120).await {
        log::warn!("Failed to cache portfolio: {}", e);
    }

    Ok(HttpResponse::Ok().json(response))
}

pub async fn get_portfolio_summary(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let cache_key = format!("portfolio_summary:{}", user_id_int);

    // Try to get from cache first
    if let Ok(Some(cached_summary)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(cached_summary));
    }

    let position_tracker = PositionTracker::new(db.get_ref().clone());

    // Get all user positions
    let positions = position_tracker
        .get_user_positions(user_id_int)
        .await
        .map_err(|e| {
            log::error!("Failed to get user positions: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve positions")
        })?;

    // Group positions by event
    let grouped_positions = position_tracker
        .get_portfolio_positions(user_id_int)
        .await
        .map_err(|e| {
            log::error!("Failed to get portfolio positions: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve portfolio")
        })?;

    let mut total_invested = Decimal::new(0, 2);
    let mut current_value = Decimal::new(0, 2);

    // Calculate totals
    for (event_id, event_positions) in &grouped_positions {
        for position in event_positions {
            // Get option details for current price
            let option = event_options::Entity::find_by_id(position.option_id)
                .one(db.get_ref())
                .await
                .map_err(|e| {
                    log::error!("Failed to get option: {}", e);
                    actix_web::error::ErrorInternalServerError("Failed to retrieve option")
                })?;

            if let Some(option) = option {
                let position_cost = position.average_price * Decimal::from(position.quantity);
                let position_value = option.current_price * Decimal::from(position.quantity);

                total_invested += position_cost;
                current_value += position_value;
            }
        }
    }

    let total_pnl = current_value - total_invested;
    let pnl_percentage = if total_invested > Decimal::new(0, 2) {
        (total_pnl / total_invested) * Decimal::new(100, 0)
    } else {
        Decimal::new(0, 2)
    };

    let summary = PortfolioSummary {
        total_positions: positions.len(),
        active_events: grouped_positions.len(),
        total_invested,
        current_value,
        total_pnl,
        pnl_percentage,
    };

    let response = json!({
        "success": true,
        "summary": summary
    });

    // Cache the response for 2 minutes
    if let Err(e) = cache_service.set(&cache_key, &response, 120).await {
        log::warn!("Failed to cache portfolio summary: {}", e);
    }

    Ok(HttpResponse::Ok().json(response))
}
