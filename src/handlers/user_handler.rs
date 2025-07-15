use actix_web::{web, Error, HttpResponse, Result};
use entity::users;
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder, QuerySelect, PaginatorTrait, ColumnTrait, QueryFilter};
use serde_json::json;
use crate::utils::pagination::{PaginationInfo, PaginatedResponse};
use crate::types::user::{ListUsersQuery, UserResponse};
use crate::utils::cache::{CacheService, create_cache_key, cache_keys};
use deadpool_redis::Pool;

pub async fn list_users(
    db: web::Data<DatabaseConnection>,
    query: web::Query<ListUsersQuery>,
) -> Result<HttpResponse, Error> {
    let mut users_query = users::Entity::find();

    // Apply filters
    if let Some(is_active) = query.is_active {
        users_query = users_query.filter(users::Column::IsActive.eq(is_active));
    }

    let page = query.pagination.get_page();
    let limit = query.pagination.get_limit();
    let offset = query.pagination.get_offset();

    // Get total count for pagination info
    let total_count = users_query
        .to_owned()
        .count(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    // Get users with pagination
    let users = users_query
        .order_by_desc(users::Column::CreatedAt)
        .limit(limit)
        .offset(offset)
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let users_response: Vec<UserResponse> = users
        .into_iter()
        .map(UserResponse::from)
        .collect();

    let pagination_info = PaginationInfo::new(page, total_count, limit);
    let response = PaginatedResponse::new(users_response, pagination_info);

    Ok(HttpResponse::Ok().json(json!({
        "message": "Users retrieved successfully",
        "status": "success",
        "data": response.data,
        "pagination": response.pagination,
    })))
}

pub async fn get_user_details(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    user_id: web::Path<i32>,
) -> Result<HttpResponse, Error> {
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let cache_key = create_cache_key(cache_keys::USER_PREFIX, &user_id.to_string());

    // Try to get from cache first
    if let Ok(Some(cached_user)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(json!({
            "message": "User details retrieved successfully".to_string(),
            "user": cached_user,
        })));
    }

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
        "role": user.role,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    });

    // Cache the user details for 10 minutes
    if let Err(e) = cache_service.set(&cache_key, &user_response, 600).await {
        log::warn!("Failed to cache user details: {}", e);
    }

    Ok(HttpResponse::Ok().json(json!({
        "message": "User details retrieved successfully".to_string(),
        "user": Some(user_response),
    })))
}

pub async fn get_current_user_details(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id: i32 = user_id_str.parse().map_err(|_| {
        actix_web::error::ErrorBadRequest("Invalid user ID")
    })?;

    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let cache_key = create_cache_key(cache_keys::USER_PREFIX, &user_id.to_string());

    // Try to get from cache first
    if let Ok(Some(cached_user)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(json!({
            "message": "Current user details retrieved successfully".to_string(),
            "user": cached_user,
        })));
    }

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
        "role": user.role,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    });

    // Cache the user details for 10 minutes
    if let Err(e) = cache_service.set(&cache_key, &user_response, 600).await {
        log::warn!("Failed to cache user details: {}", e);
    }

    Ok(HttpResponse::Ok().json(json!({
        "message": "Current user details retrieved successfully".to_string(),
        "user": Some(user_response),
    })))
}
