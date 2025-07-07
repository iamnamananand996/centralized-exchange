use actix_web::web;
use crate::handlers::health::{health_check, index};

pub fn configure_routes() -> actix_web::Scope {
    web::scope("")
        .route("/", web::get().to(index))
        .route("/health", web::get().to(health_check))
        .service(crate::routes::auth::configure_auth_routes())
        .service(crate::routes::user::configure_user_routes())
} 