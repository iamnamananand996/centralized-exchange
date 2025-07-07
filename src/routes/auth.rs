use actix_web::web;
use crate::handlers::auth_handler::{register, login};

pub fn configure_auth_routes() -> actix_web::Scope {
    web::scope("/auth")
        .route("/register", web::post().to(register))
        .route("/login", web::post().to(login))
} 