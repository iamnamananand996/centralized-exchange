use crate::middleware::auth::AuthenticatedUser;
use crate::types::event::{SettleEventRequest, SettlementPayout, SettlementResponse};
use crate::utils::auth::{check_admin_role, get_user_id};
use crate::utils::cache::{cache_keys, create_cache_key, CacheService};
use crate::websocket::server::WebSocketServer;
use actix::Addr;
use actix_web::{web, Error, HttpResponse, Result};
use chrono::Utc;
use deadpool_redis::Pool;
use entity::{event_options, events, transaction, user_positions, users};
use sea_orm::{
    prelude::Decimal, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    Set, TransactionTrait,
};
use serde_json::json;
use uuid::Uuid;

pub async fn settle_event(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    event_id: web::Path<i32>,
    req: web::Json<SettleEventRequest>,
    auth_user: web::ReqData<AuthenticatedUser>,
) -> Result<HttpResponse, Error> {
    // Check if user is admin
    if let Err(response) = check_admin_role(&auth_user) {
        return Ok(response);
    }

    let resolver_id = get_user_id(&auth_user)?;

    log::info!("Settling event {} by admin {}", event_id, resolver_id);

    // Start database transaction
    let txn = db.get_ref().begin().await.map_err(|e| {
        log::error!("Failed to start transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Transaction error")
    })?;

    // Get the event
    let event = events::Entity::find_by_id(*event_id)
        .one(&txn)
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let event = match event {
        Some(e) => e,
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "Event not found",
                "settlement": serde_json::Value::Null,
            })))
        }
    };

    // Admin users can settle any event

    // Check if event is already resolved
    if event.status == "resolved" {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Event is already resolved",
            "settlement": serde_json::Value::Null,
        })));
    }

    // Check if event has ended
    if event.end_time > Utc::now().naive_utc() && event.status != "ended" {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Event has not ended yet. You can only settle ended events.",
            "settlement": serde_json::Value::Null,
        })));
    }

    // Validate winning option exists and belongs to this event
    let winning_option = event_options::Entity::find_by_id(req.winning_option_id)
        .one(&txn)
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let winning_option = match winning_option {
        Some(o) if o.event_id == *event_id => o,
        _ => {
            return Ok(HttpResponse::BadRequest().json(json!({
                "message": "Invalid winning option ID",
                "settlement": serde_json::Value::Null,
            })))
        }
    };

    // Get all event options to mark losers
    let all_options = event_options::Entity::find()
        .filter(event_options::Column::EventId.eq(*event_id))
        .all(&txn)
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    // Mark winning and losing options
    for option in all_options {
        let mut active_option: event_options::ActiveModel = option.clone().into();
        active_option.is_winning_option = Set(Some(option.id == req.winning_option_id));
        active_option.update(&txn).await.map_err(|e| {
            log::error!("Failed to update option: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to update option")
        })?;
    }

    // Get all user positions for this event
    let positions = user_positions::Entity::find()
        .filter(user_positions::Column::EventId.eq(*event_id))
        .filter(user_positions::Column::Quantity.gt(0))
        .all(&txn)
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    // Calculate payouts
    let payout_per_share = Decimal::new(100, 2); // 1.00 per winning share
    let mut settlement_payouts = Vec::new();
    let mut total_payouts = Decimal::new(0, 2);

    for position in positions {
        // Get user details
        let user = users::Entity::find_by_id(position.user_id)
            .one(&txn)
            .await
            .map_err(|e| {
                log::error!("Database error: {}", e);
                actix_web::error::ErrorInternalServerError("Database error occurred")
            })?
            .ok_or_else(|| actix_web::error::ErrorInternalServerError("User not found"))?;

        // Get option details
        let option = event_options::Entity::find_by_id(position.option_id)
            .one(&txn)
            .await
            .map_err(|e| {
                log::error!("Database error: {}", e);
                actix_web::error::ErrorInternalServerError("Database error occurred")
            })?
            .ok_or_else(|| actix_web::error::ErrorInternalServerError("Option not found"))?;

        let is_winner = position.option_id == req.winning_option_id;
        let payout = if is_winner {
            payout_per_share * Decimal::from(position.quantity)
        } else {
            Decimal::new(0, 2)
        };

        let invested = position.average_price * Decimal::from(position.quantity);
        let profit_loss = payout - invested;

        // Update user balance if they won
        if is_winner && payout > Decimal::new(0, 2) {
            let mut active_user: users::ActiveModel = user.clone().into();
            active_user.wallet_balance = Set(active_user.wallet_balance.as_ref() + payout);
            active_user.updated_at = Set(Utc::now().naive_utc());
            active_user.update(&txn).await.map_err(|e| {
                log::error!("Failed to update user balance: {}", e);
                actix_web::error::ErrorInternalServerError("Failed to update balance")
            })?;

            // Create payout transaction record
            let transaction_record = transaction::ActiveModel {
                user_id: Set(position.user_id),
                r#type: Set("event_payout".to_string()),
                amount: Set(payout),
                balance_before: Set(user.wallet_balance),
                balance_after: Set(user.wallet_balance + payout),
                status: Set("completed".to_string()),
                reference_id: Set(format!("event_{}_{}", event_id, Uuid::new_v4())),
                created_at: Set(Utc::now().naive_utc()),
                ..Default::default()
            };
            transaction_record.insert(&txn).await.map_err(|e| {
                log::error!("Failed to create transaction record: {}", e);
                actix_web::error::ErrorInternalServerError("Failed to create transaction")
            })?;

            total_payouts += payout;
        }

        // Close the position (set quantity to 0)
        let mut active_position: user_positions::ActiveModel = position.clone().into();
        active_position.quantity = Set(0);
        active_position.updated_at = Set(Utc::now().into());
        active_position.update(&txn).await.map_err(|e| {
            log::error!("Failed to close position: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to close position")
        })?;

        settlement_payouts.push(SettlementPayout {
            user_id: position.user_id,
            username: user.username,
            option_id: position.option_id,
            option_text: option.option_text.clone(),
            shares_held: position.quantity,
            payout_per_share: if is_winner {
                payout_per_share
            } else {
                Decimal::new(0, 2)
            },
            total_payout: payout,
            profit_loss,
        });
    }

    // Update event status to resolved
    let mut active_event: events::ActiveModel = event.clone().into();
    active_event.status = Set("resolved".to_string());
    active_event.resolved_by = Set(resolver_id);
    active_event.winning_option_id = Set(req.winning_option_id);
    active_event.resolution_note = Set(req.resolution_note.clone().unwrap_or_default());
    active_event.resolved_at = Set(Utc::now().naive_utc());
    active_event.updated_at = Set(Utc::now().naive_utc());

    let updated_event = active_event.update(&txn).await.map_err(|e| {
        log::error!("Failed to update event: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to update event")
    })?;

    // Commit transaction
    txn.commit().await.map_err(|e| {
        log::error!("Failed to commit transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Transaction commit failed")
    })?;

    // Prepare response
    let settlement_response = SettlementResponse {
        event_id: updated_event.id,
        event_title: updated_event.title.clone(),
        winning_option_id: req.winning_option_id,
        winning_option_text: winning_option.option_text,
        total_payouts,
        total_positions_settled: settlement_payouts.len(),
        payouts: settlement_payouts,
        settlement_timestamp: updated_event.resolved_at,
    };

    // Invalidate caches
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let event_cache_key = create_cache_key(cache_keys::EVENT_PREFIX, &event_id.to_string());
    let _ = cache_service.delete(&event_cache_key).await;
    let _ = cache_service.delete("events:list").await;

    // Invalidate portfolio caches for all affected users
    for payout in &settlement_response.payouts {
        let portfolio_cache_key = format!("portfolio:{}", payout.user_id);
        let _ = cache_service.delete(&portfolio_cache_key).await;
    }

    // Broadcast updates
    let handlers =
        crate::websocket::handlers::WebSocketHandlers::new(db.clone(), ws_server.get_ref().clone());

    let event_id_for_broadcast = *event_id;
    tokio::spawn(async move {
        handlers
            .fetch_and_broadcast_event(event_id_for_broadcast)
            .await;
    });

    ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);

    // Notify affected users about their payouts
    for payout in &settlement_response.payouts {
        if payout.total_payout > Decimal::new(0, 2) {
            ws_server.do_send(crate::websocket::server::BroadcastTransactionsUpdate {
                user_id: payout.user_id,
            });
            ws_server.do_send(crate::websocket::server::BroadcastPortfolioUpdate {
                user_id: payout.user_id,
            });
        }
    }

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event settled successfully",
        "settlement": settlement_response,
    })))
}
