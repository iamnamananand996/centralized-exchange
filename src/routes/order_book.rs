use crate::{handlers::order_book_handler, middleware::auth::AuthMiddleware};
use actix_web::{web, Scope};

pub fn configure_order_book_routes() -> Scope {
    web::scope("/order-book")
        // Place an order (buy/sell)
        .route(
            "/orders",
            web::post()
                .to(order_book_handler::place_order)
                .wrap(AuthMiddleware),
        )
        // Cancel an order
        .route(
            "/orders/cancel",
            web::post()
                .to(order_book_handler::cancel_order)
                .wrap(AuthMiddleware),
        )
        // Get user's orders
        .route(
            "/orders/my",
            web::get()
                .to(order_book_handler::get_user_orders)
                .wrap(AuthMiddleware),
        )
        // Get order book for an event option
        .route(
            "/events/{event_id}/options/{option_id}",
            web::get().to(order_book_handler::get_order_book),
        )
        // Get market depth for an event option
        .route(
            "/events/{event_id}/options/{option_id}/depth",
            web::get().to(order_book_handler::get_market_depth),
        )
        // Get trade history for an event option
        .route(
            "/events/{event_id}/options/{option_id}/trades",
            web::get().to(order_book_handler::get_trade_history),
        )
} 