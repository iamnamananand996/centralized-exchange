use crate::types::event_option::{
    CreateEventOptionRequest, EventOptionResponse, UpdateEventOptionRequest,
};
use crate::utils::cache::{cache_keys, create_cache_key, CacheService};
use crate::utils::pagination::{PaginatedResponse, PaginationInfo, PaginationQuery};
use crate::websocket::server::WebSocketServer;
use actix::prelude::*;
use actix_web::{web, Error, HttpResponse, Result};
use deadpool_redis::Pool;
use entity::{event_options, events};
use sea_orm::{
    prelude::Decimal, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde_json::json;

pub async fn create_event_option(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    req: web::Json<CreateEventOptionRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let creator_id: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    log::info!("Creating event option for user: {}", creator_id);
    log::info!("Request: {:?}", req);

    // Verify the event exists and user is the creator
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
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "Event not found",
                "option": serde_json::Value::Null,
            })))
        }
    };

    // Check if user is the creator of the event
    if event.created_by != creator_id {
        return Ok(HttpResponse::Forbidden().json(json!({
            "message": "You can only create options for events you created",
            "option": serde_json::Value::Null,
        })));
    }

    // Check if event is still editable (not resolved or ended)
    if event.status == "resolved" || event.status == "ended" {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Cannot create options for resolved or ended events",
            "option": serde_json::Value::Null,
        })));
    }

    // Validate current_price if provided
    if let Some(price) = req.current_price {
        if price < Decimal::new(0, 2) || price > Decimal::new(10000, 2) {
            return Ok(HttpResponse::BadRequest().json(json!({
                "message": "Current price must be between 0.00 and 100.00",
                "option": serde_json::Value::Null,
            })));
        }
    }

    let new_option = event_options::ActiveModel {
        event_id: Set(req.event_id),
        option_text: Set(req.option_text.clone()),
        current_price: Set(req.current_price.unwrap_or_else(|| Decimal::new(5000, 2))), // Default 50.00
        total_backing: Set(Decimal::new(0, 2)),
        is_winning_option: Set(None),
        ..Default::default()
    };

    let option = new_option.insert(db.get_ref()).await.map_err(|e| {
        log::error!("Event option creation error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to create event option")
    })?;

    // Return success response
    let option_response = EventOptionResponse::from(option.clone());

    // Broadcast the updated event to all subscribers
    let handlers =
        crate::websocket::handlers::WebSocketHandlers::new(db.clone(), ws_server.get_ref().clone());

    // Broadcast to specific event channel
    let event_id_for_broadcast = req.event_id;
    tokio::spawn(async move {
        // Broadcast to specific event channel
        handlers
            .fetch_and_broadcast_event(event_id_for_broadcast)
            .await;
    });

    // Invalidate relevant caches
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let event_cache_key = create_cache_key(cache_keys::EVENT_PREFIX, &req.event_id.to_string());
    let options_list_key = format!("event_options:{}:*", req.event_id);

    if let Err(e) = cache_service.delete(&event_cache_key).await {
        log::warn!("Failed to invalidate event cache: {}", e);
    }
    // Note: We should ideally have a pattern-based delete, but for now we'll rely on TTL

    // Broadcast personalized events updates to all subscribers of the events channel
    ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);

    Ok(HttpResponse::Created().json(json!({
        "message": "Event option created successfully",
        "option": option_response,
    })))
}

pub async fn update_event_option(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    option_id: web::Path<i32>,
    req: web::Json<UpdateEventOptionRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let requester_id: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    // Find the event option
    let option = event_options::Entity::find_by_id(*option_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let option = match option {
        Some(o) => o,
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "Event option not found",
                "option": serde_json::Value::Null,
            })))
        }
    };

    // Find the associated event to check permissions
    let event = events::Entity::find_by_id(option.event_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let event = match event {
        Some(e) => e,
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "Associated event not found",
                "option": serde_json::Value::Null,
            })))
        }
    };

    // Check if user is the creator of the event
    if event.created_by != requester_id {
        return Ok(HttpResponse::Forbidden().json(json!({
            "message": "You can only update options for events you created",
            "option": serde_json::Value::Null,
        })));
    }

    // Check if event is still editable (not resolved or ended)
    if event.status == "resolved" || event.status == "ended" {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Cannot update options for resolved or ended events",
            "option": serde_json::Value::Null,
        })));
    }

    // Validate current_price if provided
    if let Some(price) = req.current_price {
        if price < Decimal::new(0, 2) || price > Decimal::new(10000, 2) {
            return Ok(HttpResponse::BadRequest().json(json!({
                "message": "Current price must be between 0.00 and 100.00",
                "option": serde_json::Value::Null,
            })));
        }
    }

    let mut active_option: event_options::ActiveModel = option.into();

    // Update fields if provided
    if let Some(option_text) = &req.option_text {
        active_option.option_text = Set(option_text.clone());
    }
    if let Some(current_price) = req.current_price {
        active_option.current_price = Set(current_price);
    }
    if let Some(is_winning_option) = req.is_winning_option {
        active_option.is_winning_option = Set(Some(is_winning_option));
    }

    let updated_option = active_option.update(db.get_ref()).await.map_err(|e| {
        log::error!("Event option update error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to update event option")
    })?;

    let option_response = EventOptionResponse::from(updated_option);

    // Invalidate relevant caches
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let option_cache_key = format!("event_option:{}", option_id);
    let event_cache_key = create_cache_key(cache_keys::EVENT_PREFIX, &event.id.to_string());

    if let Err(e) = cache_service.delete(&option_cache_key).await {
        log::warn!("Failed to invalidate option cache: {}", e);
    }
    if let Err(e) = cache_service.delete(&event_cache_key).await {
        log::warn!("Failed to invalidate event cache: {}", e);
    }

    // Broadcast the updated event to all subscribers
    let handlers =
        crate::websocket::handlers::WebSocketHandlers::new(db.clone(), ws_server.get_ref().clone());

    // Broadcast to specific event channel
    let event_id_for_broadcast = event.id;
    tokio::spawn(async move {
        // Broadcast to specific event channel
        handlers
            .fetch_and_broadcast_event(event_id_for_broadcast)
            .await;
    });

    // Broadcast personalized events updates to all subscribers of the events channel
    ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event option updated successfully",
        "event_option": option_response
    })))
}

pub async fn list_event_options(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    event_id: web::Path<i32>,
    query: web::Query<PaginationQuery>,
) -> Result<HttpResponse, Error> {
    let cache_service = CacheService::new(redis_pool.get_ref().clone());

    // Create cache key based on event ID and pagination
    let cache_key = format!(
        "event_options:{}:{}:{}",
        event_id,
        query.get_page(),
        query.get_limit()
    );

    // Try to get from cache first
    if let Ok(Some(cached_response)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(cached_response));
    }

    // Verify the event exists
    let event = events::Entity::find_by_id(*event_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    if event.is_none() {
        return Ok(HttpResponse::NotFound().json(json!({
            "message": "Event not found",
            "options": serde_json::Value::Null,
        })));
    }

    let options_query =
        event_options::Entity::find().filter(event_options::Column::EventId.eq(*event_id));

    // Apply pagination
    let page = query.get_page();
    let limit = query.get_limit();
    let offset = query.get_offset();

    // Get total count for pagination info
    let total_count = options_query
        .to_owned()
        .count(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    // Get options with pagination
    let options = options_query
        .order_by_asc(event_options::Column::Id)
        .limit(limit)
        .offset(offset)
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let options_response: Vec<EventOptionResponse> =
        options.into_iter().map(EventOptionResponse::from).collect();

    let pagination_info = PaginationInfo::new(page, total_count, limit);
    let response = PaginatedResponse::new(options_response, pagination_info);

    let response_json = json!({
        "message": "Event options retrieved successfully",
        "status": "success",
        "data": response.data,
        "pagination": response.pagination,
    });

    // Cache the response for 10 minutes
    if let Err(e) = cache_service.set(&cache_key, &response_json, 600).await {
        log::warn!("Failed to cache event options list: {}", e);
    }

    Ok(HttpResponse::Ok().json(response_json))
}

pub async fn get_event_option(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    option_id: web::Path<i32>,
) -> Result<HttpResponse, Error> {
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let cache_key = format!("event_option:{}", option_id);

    // Try to get from cache first
    if let Ok(Some(cached_option)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(json!({
            "message": "Event option retrieved successfully",
            "option": cached_option,
        })));
    }

    let option = event_options::Entity::find_by_id(*option_id)
        .one(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let option = match option {
        Some(o) => o,
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "Event option not found",
                "option": serde_json::Value::Null,
            })))
        }
    };

    let option_response = EventOptionResponse::from(option);

    // Cache the option for 10 minutes
    if let Err(e) = cache_service.set(&cache_key, &option_response, 600).await {
        log::warn!("Failed to cache event option: {}", e);
    }

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event option retrieved successfully",
        "option": Some(option_response),
    })))
}
