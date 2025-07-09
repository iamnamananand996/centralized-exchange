use crate::types::event_option::{
    CreateEventOptionRequest, EventOptionResponse, UpdateEventOptionRequest,
};
use crate::utils::pagination::{PaginatedResponse, PaginationInfo, PaginationQuery};
use actix_web::{web, Error, HttpResponse, Result};
use entity::{event_options, events};
use sea_orm::{
    prelude::Decimal, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde_json::json;

pub async fn create_event_option(
    db: web::Data<DatabaseConnection>,
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

    let option_response = EventOptionResponse::from(option);

    Ok(HttpResponse::Created().json(json!({
        "message": "Event option created successfully",
        "option": Some(option_response),
    })))
}

pub async fn update_event_option(
    db: web::Data<DatabaseConnection>,
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

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event option updated successfully",
        "option": Some(option_response),
    })))
}

pub async fn list_event_options(
    db: web::Data<DatabaseConnection>,
    event_id: web::Path<i32>,
    query: web::Query<PaginationQuery>,
) -> Result<HttpResponse, Error> {
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

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event options retrieved successfully",
        "status": "success",
        "data": response.data,
        "pagination": response.pagination,
    })))
}

pub async fn get_event_option(
    db: web::Data<DatabaseConnection>,
    option_id: web::Path<i32>,
) -> Result<HttpResponse, Error> {
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

    Ok(HttpResponse::Ok().json(json!({
        "message": "Event option retrieved successfully",
        "option": Some(option_response),
    })))
}
