use actix_web::web;
use crate::handlers::bet_handler;
use crate::middleware::auth::AuthMiddleware;

pub fn configure_bet_routes() -> actix_web::Scope {
    web::scope("/bets")
        .route("/place", web::post().to(bet_handler::place_bet).wrap(AuthMiddleware))
        .route("/my-bets", web::get().to(bet_handler::get_my_bets).wrap(AuthMiddleware))
}

pub fn configure_portfolio_routes() -> actix_web::Scope {
    web::scope("/portfolio")
        .route("", web::get().to(bet_handler::get_portfolio).wrap(AuthMiddleware))
}
