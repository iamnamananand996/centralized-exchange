use crate::types::event::{CreateEventRequest, EventResponse, ListEventsQuery, UpdateEventRequest};
use crate::utils::cache::{cache_keys, create_cache_key, CacheService};
use crate::utils::pagination::{PaginatedResponse, PaginationInfo};
use crate::websocket::server::WebSocketServer;
use actix::Addr;
use actix_web::{web, Error, HttpResponse, Result};
use chrono::Utc;
use deadpool_redis::Pool;
use entity::{event_options, events};
use sea_orm::{
    prelude::Decimal, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde_json::json;

pub async fn create_event(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    req: web::Json<CreateEventRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let creator_id: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    log::info!("Creating event for user: {}", creator_id);
    log::info!("Request: {:?}", req);

    // Validate end_time is in the future
    if req.end_time <= Utc::now() {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "End time must be in the future",
            "event": serde_json::Value::Null,
        })));
    }

    let new_event = events::ActiveModel {
        title: Set(req.title.clone()),
        description: Set(req.description.clone().unwrap_or_default()),
        category: Set(req
            .category
            .clone()
            .unwrap_or_else(|| "general".to_string())),
        status: Set("draft".to_string()),
        end_time: Set(req.end_time.naive_utc()),
        min_bet_amount: Set(req.min_bet_amount.unwrap_or_else(|| Decimal::new(1000, 2))), // 10.00
        max_bet_amount: Set(req
            .max_bet_amount
            .unwrap_or_else(|| Decimal::new(100000, 2))), // 1000.00
        total_volume: Set(Decimal::new(0, 2)),
        image_url: Set(req.image_url.clone().unwrap_or_default()),
        created_by: Set(creator_id),
        resolved_by: Set(creator_id), // Using 0 as default for nullable int fields
        winning_option_id: Set(0),    // Using 0 as default for nullable int fields
        resolution_note: Set("".to_string()),
        resolved_at: Set(chrono::Utc::now().naive_utc()), // Using current time as default, will be properly set when resolved
        ..Default::default()
    };

    let event = new_event.insert(db.get_ref()).await.map_err(|e| {
        log::error!("Event creation error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to create event")
    })?;

    // Fetch options for the event (will be empty for new events)
    let options = event_options::Entity::find()
        .filter(event_options::Column::EventId.eq(event.id))
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let event_response = EventResponse::from((event.clone(), options));

    // Invalidate events list cache
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    // Note: We should ideally have a pattern-based delete, but for now we'll rely on TTL
    if let Err(e) = cache_service.delete("events:list").await {
        log::warn!("Failed to invalidate events list cache: {}", e);
    }

    // Broadcast to specific event channel
    let handlers =
        crate::websocket::handlers::WebSocketHandlers::new(db.clone(), ws_server.get_ref().clone());

    let event_id_for_broadcast = event.id;
    tokio::spawn(async move {
        // Broadcast to specific event channel
        handlers
            .fetch_and_broadcast_event(event_id_for_broadcast)
            .await;
    });

    // Broadcast the new event to all subscribers of the events channel
    ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);

    Ok(HttpResponse::Created().json(json!({
        "message": "Event created successfully",
        "event": Some(event_response),
    })))
}

pub async fn update_event(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    ws_server: web::Data<Addr<WebSocketServer>>,
    event_id: web::Path<i32>,
    req: web::Json<UpdateEventRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let requester_id: i32 = user_id_str
        .parse()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))?;

    // Find the event
    let event = events::Entity::find_by_id(*event_id)
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
                "event": serde_json::Value::Null,
            })))
        }
    };

    // Check if user is the creator of the event
    if event.created_by != requester_id {
        return Ok(HttpResponse::Forbidden().json(json!({
            "message": "You can only update events you created",
            "event": serde_json::Value::Null,
        })));
    }

    // Check if event is still editable (not resolved or ended)
    if event.status == "resolved" || event.status == "ended" {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Cannot update resolved or ended events",
            "event": serde_json::Value::Null,
        })));
    }

    // Validate end_time if provided
    if let Some(end_time) = &req.end_time {
        if *end_time <= Utc::now() {
            return Ok(HttpResponse::BadRequest().json(json!({
                "message": "End time must be in the future",
                "event": serde_json::Value::Null,
            })));
        }
    }

    let mut active_event: events::ActiveModel = event.into();

    // Update fields if provided
    if let Some(title) = &req.title {
        active_event.title = Set(title.clone());
    }
    if let Some(description) = &req.description {
        active_event.description = Set(description.clone());
    }
    if let Some(category) = &req.category {
        active_event.category = Set(category.clone());
    }
    if let Some(status) = &req.status {
        // Validate status
        if ![
            "draft",
            "active",
            "paused",
            "ended",
            "resolved",
            "cancelled",
        ]
        .contains(&status.as_str())
        {
            return Ok(HttpResponse::BadRequest().json(json!({
                "message": "Invalid status. Must be one of: draft, active, paused, ended, resolved, cancelled",
                "event": serde_json::Value::Null,
            })));
        }
        active_event.status = Set(status.clone());
    }
    if let Some(end_time) = &req.end_time {
        active_event.end_time = Set(end_time.naive_utc());
    }
    if let Some(min_bet_amount) = &req.min_bet_amount {
        active_event.min_bet_amount = Set(*min_bet_amount);
    }
    if let Some(max_bet_amount) = &req.max_bet_amount {
        active_event.max_bet_amount = Set(*max_bet_amount);
    }
    if let Some(image_url) = &req.image_url {
        active_event.image_url = Set(image_url.clone());
    }

    let updated_event = active_event.update(db.get_ref()).await.map_err(|e| {
        log::error!("Event update error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to update event")
    })?;

    // Fetch options for the updated event
    let options = event_options::Entity::find()
        .filter(event_options::Column::EventId.eq(updated_event.id))
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let event_response = EventResponse::from((updated_event.clone(), options));

    // Invalidate relevant caches
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let event_cache_key = create_cache_key(cache_keys::EVENT_PREFIX, &event_id.to_string());
    if let Err(e) = cache_service.delete(&event_cache_key).await {
        log::warn!("Failed to invalidate event cache: {}", e);
    }
    if let Err(e) = cache_service.delete("events:list").await {
        log::warn!("Failed to invalidate events list cache: {}", e);
    }

    // Broadcast to specific event channel
    let handlers =
        crate::websocket::handlers::WebSocketHandlers::new(db.clone(), ws_server.get_ref().clone());

    let event_id_for_broadcast = updated_event.id;
    tokio::spawn(async move {
        // Broadcast to specific event channel (event:24)
        handlers
            .fetch_and_broadcast_event(event_id_for_broadcast)
            .await;
    });

    // Broadcast the updated event to all subscribers of the events channel
    ws_server.do_send(crate::websocket::server::BroadcastEventsUpdate);

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event updated successfully",
        "event": Some(event_response),
    })))
}

pub async fn list_events(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    query: web::Query<ListEventsQuery>,
) -> Result<HttpResponse, Error> {
    let cache_service = CacheService::new(redis_pool.get_ref().clone());

    // Create cache key based on query parameters
    let cache_key = format!(
        "events:list:{}:{}:{}:{}",
        query.status.as_deref().unwrap_or("all"),
        query.category.as_deref().unwrap_or("all"),
        query.pagination.get_page(),
        query.pagination.get_limit()
    );

    // Try to get from cache first
    if let Ok(Some(cached_response)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(cached_response));
    }

    let mut events_query = events::Entity::find();

    // Apply filters
    if let Some(status) = &query.status {
        events_query = events_query.filter(events::Column::Status.eq(status));
    }
    if let Some(category) = &query.category {
        events_query = events_query.filter(events::Column::Category.eq(category));
    }

    // Apply pagination
    let page = query.pagination.get_page();
    let limit = query.pagination.get_limit();
    let offset = query.pagination.get_offset();

    // Get total count for pagination info
    let total_count = events_query
        .to_owned()
        .count(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    // Get events with pagination
    let events = events_query
        .order_by_desc(events::Column::CreatedAt)
        .limit(limit)
        .offset(offset)
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    // Fetch options for all events
    let mut events_response: Vec<EventResponse> = Vec::new();
    for event in events {
        let options = event_options::Entity::find()
            .filter(event_options::Column::EventId.eq(event.id))
            .all(db.get_ref())
            .await
            .map_err(|e| {
                log::error!("Database error: {}", e);
                actix_web::error::ErrorInternalServerError("Database error occurred")
            })?;

        events_response.push(EventResponse::from((event, options)));
    }

    let pagination_info = PaginationInfo::new(page, total_count, limit);
    let response = PaginatedResponse::new(events_response, pagination_info);

    let response_json = json!({
        "message": "Events retrieved successfully",
        "status": "success",
        "data": response.data,
        "pagination": response.pagination,
    });

    // Cache the response for 5 minutes
    if let Err(e) = cache_service.set(&cache_key, &response_json, 300).await {
        log::warn!("Failed to cache events list: {}", e);
    }

    Ok(HttpResponse::Ok().json(response_json))
}

pub async fn get_event(
    db: web::Data<DatabaseConnection>,
    redis_pool: web::Data<Pool>,
    event_id: web::Path<i32>,
) -> Result<HttpResponse, Error> {
    let cache_service = CacheService::new(redis_pool.get_ref().clone());
    let cache_key = create_cache_key(cache_keys::EVENT_PREFIX, &event_id.to_string());

    // Try to get from cache first
    if let Ok(Some(cached_event)) = cache_service.get::<serde_json::Value>(&cache_key).await {
        return Ok(HttpResponse::Ok().json(json!({
            "message": "Event retrieved successfully",
            "event": cached_event,
        })));
    }

    let event = events::Entity::find_by_id(*event_id)
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
                "event": serde_json::Value::Null,
            })))
        }
    };

    let options = event_options::Entity::find()
        .filter(event_options::Column::EventId.eq(event.id))
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let event_response = EventResponse::from((event, options));

    // Cache the event for 10 minutes
    if let Err(e) = cache_service.set(&cache_key, &event_response, 600).await {
        log::warn!("Failed to cache event: {}", e);
    }

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event retrieved successfully",
        "event": Some(event_response),
    })))
}
