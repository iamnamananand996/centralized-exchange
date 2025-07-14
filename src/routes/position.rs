use actix_web::web;
use crate::handlers::position_handler;
use crate::middleware::auth::AuthMiddleware;

pub fn configure_position_routes() -> actix_web::Scope {
    web::scope("/positions")
        .route("/my", web::get().to(position_handler::get_my_positions).wrap(AuthMiddleware))
        .route("/{event_id}/{option_id}", web::get().to(position_handler::get_position))
} 