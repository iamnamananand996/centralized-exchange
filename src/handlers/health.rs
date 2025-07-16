use crate::types::response::ApiResponse;
use actix_web::{HttpResponse, Result};

pub async fn health_check() -> Result<HttpResponse> {
    let response = ApiResponse {
        message: "Centralized Exchange API is running".to_string(),
        status: "healthy".to_string(),
    };
    Ok(HttpResponse::Ok().json(response))
}

pub async fn index() -> Result<HttpResponse> {
    let response = ApiResponse {
        message: "Welcome to Centralized Exchange API".to_string(),
        status: "success".to_string(),
    };
    Ok(HttpResponse::Ok().json(response))
}
