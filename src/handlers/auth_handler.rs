use actix_web::{web, Error, HttpResponse, Result};
use bcrypt::{hash, verify, DEFAULT_COST};
use entity::users;
use sea_orm::{
    prelude::Decimal, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    Set,
};
use serde::Deserialize;
use serde_json::json;

use crate::utils::jwt::create_jwt_token;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub phone: Option<String>,
    pub password: String,
    pub full_name: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

pub async fn register(
    db: web::Data<DatabaseConnection>,
    req: web::Json<RegisterRequest>,
) -> Result<HttpResponse, Error> {
    // Check if user already exists
    let existing_user = users::Entity::find()
        .filter(users::Column::Email.eq(&req.email))
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if existing_user.is_some() {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "User with this email already exists".to_string(),
            "token": serde_json::Value::Null,
            "user": serde_json::Value::Null,
        })));
    }

    // Check if username is taken
    let existing_username = users::Entity::find()
        .filter(users::Column::Username.eq(&req.username))
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if existing_username.is_some() {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Username is already taken".to_string(),
            "token": serde_json::Value::Null,
            "user": serde_json::Value::Null,
        })));
    }

    // Hash password
    let password_hash = hash(&req.password, DEFAULT_COST).map_err(|e| {
        log::error!("Password hashing error: {}", e);
        actix_web::error::ErrorInternalServerError("Error processing password")
    })?;

    // Create new user
    let new_user = users::ActiveModel {
        username: Set(req.username.clone()),
        email: Set(req.email.clone()),
        phone: Set(req.phone.clone()),
        password_hash: Set(password_hash),
        full_name: Set(req.full_name.clone()),
        wallet_balance: Set(Decimal::from(0)),
        is_active: Set(true),
        ..Default::default()
    };

    let user = new_user.insert(db.get_ref()).await.map_err(|e| {
        log::error!("User creation error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to create user")
    })?;

    // Create JWT token
    let token = create_jwt_token(&user.id.to_string()).map_err(|e| {
        log::error!("JWT token creation error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to create authentication token")
    })?;

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

    Ok(HttpResponse::Created().json(json!({
        "message": "User registered successfully".to_string(),
        "token": Some(token),
        "user": Some(user_response),
    })))
}

pub async fn login(
    db: web::Data<DatabaseConnection>,
    req: web::Json<LoginRequest>,
) -> Result<HttpResponse, Error> {
    // Find user by email
    let user = users::Entity::find()
        .filter(users::Column::Email.eq(&req.email))
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let user = match user {
        Some(u) => u,
        None => {
            return Ok(HttpResponse::Unauthorized().json(json!({
                "message": "Invalid email or password".to_string(),
                "token": serde_json::Value::Null,
                "user": serde_json::Value::Null,
            })))
        }
    };

    // Check if user is active
    if !user.is_active {
        return Ok(HttpResponse::Unauthorized().json(json!({
            "message": "Account is deactivated".to_string(),
            "token": serde_json::Value::Null,
            "user": serde_json::Value::Null,
        })));
    }

    // Verify password
    let is_valid = verify(&req.password, &user.password_hash).map_err(|e| {
        log::error!("Password verification error: {}", e);
        actix_web::error::ErrorInternalServerError("Error verifying password")
    })?;

    if !is_valid {
        return Ok(HttpResponse::Unauthorized().json(json!({
            "message": "Invalid email or password".to_string(),
            "token": serde_json::Value::Null,
            "user": serde_json::Value::Null,
        })));
    }

    // Create JWT token
    let token = create_jwt_token(&user.id.to_string()).map_err(|e| {
        log::error!("JWT token creation error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to create authentication token")
    })?;

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
        "message": "Login successful".to_string(),
        "token": Some(token),
        "user": Some(user_response),
    })))
}
