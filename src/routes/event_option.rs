use crate::handlers::event_option_handler::*;
use crate::middleware::auth::AuthMiddleware;
use actix_web::web;

pub fn configure_event_option_routes() -> actix_web::Scope {
    web::scope("/event-options")
        .route(
            "/create",
            web::post().to(create_event_option).wrap(AuthMiddleware),
        )
        .route(
            "/{option_id}",
            web::put().to(update_event_option).wrap(AuthMiddleware),
        )
        .route("/{option_id}", web::get().to(get_event_option))
}
