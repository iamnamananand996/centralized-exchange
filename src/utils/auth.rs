use actix_web::{web, HttpResponse, Result, Error};
use serde_json::json;
use crate::middleware::auth::AuthenticatedUser;

/// Check if the authenticated user has admin role
pub fn check_admin_role(auth_user: &web::ReqData<AuthenticatedUser>) -> Result<(), HttpResponse> {
    if auth_user.role != "admin" {
        return Err(HttpResponse::Forbidden().json(json!({
            "message": "Only admin users can perform this action",
            "status": "error"
        })));
    }
    Ok(())
}

/// Get user ID from authenticated user data
pub fn get_user_id(auth_user: &web::ReqData<AuthenticatedUser>) -> Result<i32, Error> {
    auth_user.id.parse::<i32>()
        .map_err(|_| actix_web::error::ErrorBadRequest("Invalid user ID"))
} 