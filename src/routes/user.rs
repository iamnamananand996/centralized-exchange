use crate::handlers::user_handler::{get_current_user_details, get_user_details};
use crate::middleware::auth::AuthMiddleware;
use actix_web::web;

pub fn configure_user_routes() -> actix_web::Scope {
    web::scope("/users").service(
        web::scope("")
            .route("/me", web::get().to(get_current_user_details).wrap(AuthMiddleware))
            .route("/{user_id}", web::get().to(get_user_details))
    )
}
