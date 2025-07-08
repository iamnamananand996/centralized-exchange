use actix_web::{web, Error, HttpResponse, Result};
use chrono::{DateTime, Utc};
use entity::events;
use sea_orm::{
    prelude::Decimal, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, PaginatorTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::utils::pagination::{PaginationQuery, PaginationInfo, PaginatedResponse};

#[derive(Deserialize, Debug)]
pub struct CreateEventRequest {
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub end_time: DateTime<Utc>,
    pub min_bet_amount: Option<Decimal>,
    pub max_bet_amount: Option<Decimal>,
    pub image_url: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateEventRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub end_time: Option<DateTime<Utc>>,
    pub min_bet_amount: Option<Decimal>,
    pub max_bet_amount: Option<Decimal>,
    pub image_url: Option<String>,
}

#[derive(Deserialize)]
pub struct ListEventsQuery {
    pub status: Option<String>,
    pub category: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Serialize)]
pub struct EventResponse {
    pub id: i32,
    pub title: String,
    pub description: String,
    pub category: String,
    pub status: String,
    pub end_time: chrono::NaiveDateTime,
    pub min_bet_amount: Decimal,
    pub max_bet_amount: Decimal,
    pub total_volume: Decimal,
    pub image_url: String,
    pub created_by: i32,
    pub resolved_by: Option<i32>,
    pub winning_option_id: Option<i32>,
    pub resolution_note: String,
    pub resolved_at: Option<chrono::NaiveDateTime>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

impl From<events::Model> for EventResponse {
    fn from(event: events::Model) -> Self {
        Self {
            id: event.id,
            title: event.title,
            description: event.description,
            category: event.category,
            status: event.status,
            end_time: event.end_time,
            min_bet_amount: event.min_bet_amount,
            max_bet_amount: event.max_bet_amount,
            total_volume: event.total_volume,
            image_url: event.image_url,
            created_by: event.created_by,
            resolved_by: if event.resolved_by == 0 { None } else { Some(event.resolved_by) },
            winning_option_id: if event.winning_option_id == 0 { None } else { Some(event.winning_option_id) },
            resolution_note: event.resolution_note,
            resolved_at: if event.resolved_at == chrono::NaiveDateTime::default() { None } else { Some(event.resolved_at) },
            created_at: event.created_at,
            updated_at: event.updated_at,
        }
    }
}

pub async fn create_event(
    db: web::Data<DatabaseConnection>,
    req: web::Json<CreateEventRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let creator_id: i32 = user_id_str.parse().map_err(|_| {
        actix_web::error::ErrorBadRequest("Invalid user ID")
    })?;

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
        category: Set(req.category.clone().unwrap_or_else(|| "general".to_string())),
        status: Set("draft".to_string()),
        end_time: Set(req.end_time.naive_utc()),
        min_bet_amount: Set(req.min_bet_amount.unwrap_or_else(|| Decimal::new(1000, 2))), // 10.00
        max_bet_amount: Set(req.max_bet_amount.unwrap_or_else(|| Decimal::new(100000, 2))), // 1000.00
        total_volume: Set(Decimal::new(0, 2)),
        image_url: Set(req.image_url.clone().unwrap_or_default()),
        created_by: Set(creator_id),
        resolved_by: Set(creator_id), // Using 0 as default for nullable int fields
        winning_option_id: Set(0), // Using 0 as default for nullable int fields
        resolution_note: Set("".to_string()),
        resolved_at: Set(chrono::Utc::now().naive_utc()), // Using current time as default, will be properly set when resolved
        ..Default::default()
    };

    let event = new_event.insert(db.get_ref()).await.map_err(|e| {
        log::error!("Event creation error: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to create event")
    })?;

    let event_response = json!({
        "id": event.id,
        "title": event.title,
        "description": event.description,
        "category": event.category,
        "status": event.status,
        "end_time": event.end_time,
        "min_bet_amount": event.min_bet_amount,
        "max_bet_amount": event.max_bet_amount,
        "total_volume": event.total_volume,
        "image_url": event.image_url,
        "created_by": event.created_by,
        "resolved_by": if event.resolved_by == 0 { serde_json::Value::Null } else { json!(event.resolved_by) },
        "winning_option_id": if event.winning_option_id == 0 { serde_json::Value::Null } else { json!(event.winning_option_id) },
        "resolution_note": event.resolution_note,
        "resolved_at": event.resolved_at,
        "created_at": event.created_at,
        "updated_at": event.updated_at,
    });

    Ok(HttpResponse::Created().json(json!({
        "message": "Event created successfully",
        "event": Some(event_response),
    })))
}

pub async fn update_event(
    db: web::Data<DatabaseConnection>,
    event_id: web::Path<i32>,
    req: web::Json<UpdateEventRequest>,
    user_id: web::ReqData<String>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let requester_id: i32 = user_id_str.parse().map_err(|_| {
        actix_web::error::ErrorBadRequest("Invalid user ID")
    })?;

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
        if !["draft", "active", "paused", "ended", "resolved", "cancelled"].contains(&status.as_str()) {
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

    let event_response = json!({
        "id": updated_event.id,
        "title": updated_event.title,
        "description": updated_event.description,
        "category": updated_event.category,
        "status": updated_event.status,
        "end_time": updated_event.end_time,
        "min_bet_amount": updated_event.min_bet_amount,
        "max_bet_amount": updated_event.max_bet_amount,
        "total_volume": updated_event.total_volume,
        "image_url": updated_event.image_url,
        "created_by": updated_event.created_by,
        "resolved_by": if updated_event.resolved_by == requester_id { serde_json::Value::Null } else { json!(updated_event.resolved_by) },
        "winning_option_id": if updated_event.winning_option_id == 0 { serde_json::Value::Null } else { json!(updated_event.winning_option_id) },
        "resolution_note": updated_event.resolution_note,
        "resolved_at": updated_event.resolved_at,
        "created_at": updated_event.created_at,
        "updated_at": updated_event.updated_at,
    });

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event updated successfully",
        "event": Some(event_response),
    })))
}

pub async fn list_events(
    db: web::Data<DatabaseConnection>,
    query: web::Query<ListEventsQuery>,
) -> Result<HttpResponse, Error> {
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

    let events_response: Vec<EventResponse> = events
        .into_iter()
        .map(EventResponse::from)
        .collect();

    let pagination_info = PaginationInfo::new(page, total_count, limit);
    let response = PaginatedResponse::new(events_response, pagination_info);

    Ok(HttpResponse::Ok().json(json!({
        "message": "Events retrieved successfully",
        "status": "success",
        "data": response.data,
        "pagination": response.pagination,
    })))
}

pub async fn get_event(
    db: web::Data<DatabaseConnection>,
    event_id: web::Path<i32>,
) -> Result<HttpResponse, Error> {
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

    let event_response = json!({
        "id": event.id,
        "title": event.title,
        "description": event.description,
        "category": event.category,
        "status": event.status,
        "end_time": event.end_time,
        "min_bet_amount": event.min_bet_amount,
        "max_bet_amount": event.max_bet_amount,
        "total_volume": event.total_volume,
        "image_url": event.image_url,
        "created_by": event.created_by,
        "resolved_by": if event.resolved_by == 0 { serde_json::Value::Null } else { json!(event.resolved_by) },
        "winning_option_id": if event.winning_option_id == 0 { serde_json::Value::Null } else { json!(event.winning_option_id) },
        "resolution_note": event.resolution_note,
        "resolved_at": event.resolved_at,
        "created_at": event.created_at,
        "updated_at": event.updated_at,
    });

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event retrieved successfully",
        "event": Some(event_response),
    })))
}
