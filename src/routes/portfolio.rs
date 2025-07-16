use crate::handlers::portfolio_handler;
use crate::middleware::auth::AuthMiddleware;
use actix_web::web;

pub fn configure_portfolio_routes() -> actix_web::Scope {
    web::scope("/portfolio")
        .route(
            "",
            web::get()
                .to(portfolio_handler::get_portfolio)
                .wrap(AuthMiddleware),
        )
        .route(
            "/summary",
            web::get()
                .to(portfolio_handler::get_portfolio_summary)
                .wrap(AuthMiddleware),
        )
}
