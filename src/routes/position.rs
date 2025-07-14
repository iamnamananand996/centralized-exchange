use actix_web::web;
use crate::handlers::position_handler;
use crate::middleware::auth::AuthMiddleware;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/positions")
            .wrap(AuthMiddleware)
            .route("/my", web::get().to(position_handler::get_my_positions))
            .route("/{event_id}/{option_id}", web::get().to(position_handler::get_position))
    );
} 