use actix_web::web;
use crate::handlers::event_handler::{create_event, update_event, list_events, get_event};
use crate::middleware::auth::AuthMiddleware;

pub fn configure_event_routes() -> actix_web::Scope {
    web::scope("/events")
        .route("", web::get().to(list_events))
        .route("/create", web::post().to(create_event).wrap(AuthMiddleware))
        .route("/{event_id}", web::get().to(get_event))
        .route("/{event_id}", web::put().to(update_event).wrap(AuthMiddleware))
}
