use actix::prelude::{
    fut, Actor, ActorContext, ActorFutureExt, AsyncContext, ContextFutureSpawner, Handler, Running,
    StreamHandler, WrapFuture,
};
use actix::Addr;
use actix_web_actors::ws;
use chrono::Utc;
use log::{error, warn};
use serde_json;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::types::websocket::{
    PreSerializedMessage, SubscriptionChannel, WebSocketMessage, WebSocketRequest,
    WebSocketResponse,
};
use crate::websocket::server::{Connect, Disconnect, Subscribe, Unsubscribe, WebSocketServer};

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

pub struct WebSocketSession {
    /// unique session id
    pub id: usize,
    /// Client must send ping at least once per 60 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    pub hb: Instant,
    /// WebSocket server
    pub server: Addr<WebSocketServer>,
    /// User ID if authenticated
    pub user_id: Option<i32>,
    /// Subscribed channels
    pub subscriptions: HashSet<SubscriptionChannel>,
}

impl WebSocketSession {
    pub fn new(server: Addr<WebSocketServer>, user_id: Option<i32>) -> Self {
        Self {
            id: 0,
            hb: Instant::now(),
            server,
            user_id,
            subscriptions: HashSet::new(),
        }
    }

    /// helper method that sends ping to client every 30 seconds.
    ///
    /// also this method checks heartbeats from client
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                // heartbeat timed out
                warn!("WebSocket Client heartbeat failed, disconnecting!");

                // notify WebSocket server
                act.server.do_send(Disconnect { id: act.id });

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            // Send WebSocket protocol ping (not application-level message)
            ctx.ping(b"");
        });
    }

    /// Handle subscribe request
    fn handle_subscribe(
        &mut self,
        channel: String,
        params: Option<serde_json::Value>,
    ) -> Option<WebSocketResponse> {
        if let Some(channel_enum) = SubscriptionChannel::from_string(&channel) {
            // Check if user has permission to subscribe to this channel
            match &channel_enum {
                SubscriptionChannel::Transactions | SubscriptionChannel::Portfolio => {
                    if self.user_id.is_none() {
                        return Some(WebSocketResponse::error(
                            "Authentication required for this channel".to_string(),
                        ));
                    }
                }
                _ => {}
            }

            self.subscriptions.insert(channel_enum.clone());

            // Notify server about subscription
            self.server.do_send(Subscribe {
                id: self.id,
                channel: channel_enum.clone(),
                user_id: self.user_id,
                params: params.clone(),
            });

            // Don't send subscription confirmation - data will come from the server
            None
        } else {
            Some(WebSocketResponse::error(format!(
                "Invalid channel: {}",
                channel
            )))
        }
    }

    /// Handle unsubscribe request
    fn handle_unsubscribe(&mut self, channel: String) -> Option<WebSocketResponse> {
        if let Some(channel_enum) = SubscriptionChannel::from_string(&channel) {
            self.subscriptions.remove(&channel_enum);

            // Notify server about unsubscription
            self.server.do_send(Unsubscribe {
                id: self.id,
                channel: channel_enum.clone(),
            });

            // Return simple confirmation
            Some(WebSocketResponse::success(WebSocketMessage::Unsubscribe {
                channel: channel_enum.to_string(),
            }))
        } else {
            Some(WebSocketResponse::error(format!(
                "Invalid channel: {}",
                channel
            )))
        }
    }
}

impl Actor for WebSocketSession {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We register ws session with WebSocketServer
    fn started(&mut self, ctx: &mut Self::Context) {
        // we'll start heartbeat process on session start.
        self.hb(ctx);

        // register self in WebSocket server. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        let addr = ctx.address();
        self.server
            .send(Connect {
                id: self.id,
                addr: addr.clone().recipient::<PreSerializedMessage>(),
                user_id: self.user_id,
            })
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Ok(res) => act.id = res,
                    _ => ctx.stop(),
                }
                fut::ready(())
            })
            .wait(ctx);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // notify WebSocket server
        self.server.do_send(Disconnect { id: self.id });
        Running::Stop
    }
}

/// Handle messages from WebSocket server
impl Handler<WebSocketMessage> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, msg: WebSocketMessage, ctx: &mut Self::Context) {
        let response = WebSocketResponse::success(msg);
        if let Ok(json) = serde_json::to_string(&response) {
            ctx.text(json);
        }
    }
}

/// WebSocket message handler
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(_) => {
                ctx.stop();
                return;
            }
            Ok(msg) => msg,
        };

        match msg {
            ws::Message::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            }
            ws::Message::Text(text) => {
                self.hb = Instant::now();

                // Parse the incoming message
                let request: WebSocketRequest = match serde_json::from_str(&text) {
                    Ok(req) => req,
                    Err(e) => {
                        error!("Failed to parse WebSocket message: {}", e);
                        let response = WebSocketResponse::error(format!("Invalid JSON: {}", e));
                        if let Ok(json) = serde_json::to_string(&response) {
                            ctx.text(json);
                        }
                        return;
                    }
                };

                // Handle the request
                let response = match request.r#type.as_str() {
                    "subscribe" => {
                        if let Some(channel) = request.channel {
                            self.handle_subscribe(channel, request.params)
                        } else {
                            Some(WebSocketResponse::error(
                                "Channel required for subscribe".to_string(),
                            ))
                        }
                    }
                    "unsubscribe" => {
                        if let Some(channel) = request.channel {
                            self.handle_unsubscribe(channel)
                        } else {
                            Some(WebSocketResponse::error(
                                "Channel required for unsubscribe".to_string(),
                            ))
                        }
                    }
                    "ping" => Some(WebSocketResponse::success(WebSocketMessage::Pong {
                        timestamp: Utc::now(),
                    })),
                    _ => Some(WebSocketResponse::error(format!(
                        "Unknown message type: {}",
                        request.r#type
                    ))),
                };

                // Only send response if there is one
                if let Some(resp) = response {
                    if let Ok(json) = serde_json::to_string(&resp) {
                        ctx.text(json);
                    }
                }
            }
            ws::Message::Binary(_) => {
                warn!("Unexpected binary message");
            }
            ws::Message::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            ws::Message::Continuation(_) => {
                ctx.stop();
            }
            ws::Message::Nop => (),
        }
    }
}

/// Handle pre-serialized messages (for efficient broadcasting)
impl Handler<PreSerializedMessage> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, msg: PreSerializedMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}
