use crate::utils::user::extract_user_id_from_headers;
use crate::websocket::{server::WebSocketServer, session::WebSocketSession};
use actix::Addr;
use actix_web::{web, HttpRequest, HttpResponse, Result};
use actix_web_actors::ws;
use log::info;

pub async fn websocket_route(
    req: HttpRequest,
    stream: web::Payload,
    ws_server: web::Data<Addr<WebSocketServer>>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("WebSocket connection attempt");

    // Try to extract user ID from token (optional for WebSocket)
    let user_id = extract_user_id_from_headers(&req);

    let session = WebSocketSession::new(ws_server.get_ref().clone(), user_id);
    let resp = ws::start(session, &req, stream)?;

    info!("WebSocket connection established for user: {:?}", user_id);
    Ok(resp)
}

pub fn configure_websocket_routes() -> actix_web::Scope {
    web::scope("/ws").route("/connect", web::get().to(websocket_route))
}
