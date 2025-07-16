use crate::handlers::transaction_handler::{
    deposit_money, get_transaction_history, withdraw_money,
};
use crate::middleware::auth::AuthMiddleware;
use actix_web::web;

pub fn configure_transaction_routes() -> actix_web::Scope {
    web::scope("/wallet").service(
        web::scope("")
            .route(
                "/deposit",
                web::post().to(deposit_money).wrap(AuthMiddleware),
            )
            .route(
                "/withdraw",
                web::post().to(withdraw_money).wrap(AuthMiddleware),
            )
            .route(
                "/transactions",
                web::get().to(get_transaction_history).wrap(AuthMiddleware),
            ),
    )
}
