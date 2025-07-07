use actix_web::{web, Error, HttpResponse, Result};
use entity::users;
use sea_orm::{DatabaseConnection, EntityTrait};
use serde_json::json;

pub async fn get_user_details(
    db: web::Data<DatabaseConnection>,
    user_id: web::Path<i32>,
) -> Result<HttpResponse, Error> {
    // Find user by ID
    let user = users::Entity::find_by_id(*user_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let user = match user {
        Some(u) => u,
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "User not found".to_string(),
                "user": serde_json::Value::Null,
            })))
        }
    };

    // Check if user is active
    if !user.is_active {
        return Ok(HttpResponse::NotFound().json(json!({
            "message": "User account is deactivated".to_string(),
            "user": serde_json::Value::Null,
        })));
    }

    let user_response = json!({
        "id": user.id,
        "username": user.username,
        "email": user.email,
        "phone": user.phone,
        "full_name": user.full_name,
        "wallet_balance": user.wallet_balance,
        "is_active": user.is_active,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    });

    Ok(HttpResponse::Ok().json(json!({
        "message": "User details retrieved successfully".to_string(),
        "user": Some(user_response),
    })))
}

pub async fn get_current_user_details(
    db: web::Data<DatabaseConnection>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id: i32 = user_id_str.parse().map_err(|_| {
        actix_web::error::ErrorBadRequest("Invalid user ID")
    })?;

    // Find user by ID
    let user = users::Entity::find_by_id(user_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let user = match user {
        Some(u) => u,
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "User not found".to_string(),
                "user": serde_json::Value::Null,
            })))
        }
    };

    // Check if user is active
    if !user.is_active {
        return Ok(HttpResponse::NotFound().json(json!({
            "message": "User account is deactivated".to_string(),
            "user": serde_json::Value::Null,
        })));
    }

    let user_response = json!({
        "id": user.id,
        "username": user.username,
        "email": user.email,
        "phone": user.phone,
        "full_name": user.full_name,
        "wallet_balance": user.wallet_balance,
        "is_active": user.is_active,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    });

    Ok(HttpResponse::Ok().json(json!({
        "message": "Current user details retrieved successfully".to_string(),
        "user": Some(user_response),
    })))
}
