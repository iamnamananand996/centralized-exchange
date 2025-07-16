use crate::order_book::position_tracker::PositionTracker;
use actix_web::{web, Error, HttpResponse, Result};
use sea_orm::DatabaseConnection;
use serde_json::json;

pub async fn get_my_positions(
    db: web::Data<DatabaseConnection>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    let position_tracker = PositionTracker::new(db.get_ref().clone());

    let positions = position_tracker
        .get_user_positions(user_id_int)
        .await
        .map_err(|e| {
            log::error!("Failed to get user positions: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve positions")
        })?;

    let portfolio = position_tracker
        .get_portfolio_positions(user_id_int)
        .await
        .map_err(|e| {
            log::error!("Failed to get portfolio positions: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve portfolio")
        })?;

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "positions": positions,
        "portfolio": portfolio
    })))
}

pub async fn get_position(
    db: web::Data<DatabaseConnection>,
    path: web::Path<(i32, i32)>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let (event_id, option_id) = path.into_inner();
    let user_id_str = &*user_id;
    let user_id_int: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    let position_tracker = PositionTracker::new(db.get_ref().clone());

    let position = position_tracker
        .get_user_position(user_id_int, event_id, option_id)
        .await
        .map_err(|e| {
            log::error!("Failed to get user position: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to retrieve position")
        })?;

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "position": position
    })))
}
