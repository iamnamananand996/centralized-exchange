use actix::prelude::{Actor, AsyncContext, Context, Handler, Message, Recipient};
use actix_web::web;
use deadpool_redis::Pool;
use log::info;
use sea_orm::DatabaseConnection;
use serde_json;
use std::collections::{HashMap, HashSet};

use crate::types::websocket::{PreSerializedMessage, SubscriptionChannel, WebSocketMessage};

/// WebSocket server manages all connections and subscriptions
#[derive(Default)]
pub struct WebSocketServer {
    /// Map of session id to session address
    sessions: HashMap<usize, Recipient<PreSerializedMessage>>,
    /// Map of user id to session ids
    user_sessions: HashMap<i32, HashSet<usize>>,
    /// Map of channels to subscribed session ids
    subscriptions: HashMap<SubscriptionChannel, HashSet<usize>>,
    /// Map of (session_id, channel) to subscription parameters
    subscription_params: HashMap<(usize, SubscriptionChannel), serde_json::Value>,
    /// Counter for generating unique session ids
    session_counter: usize,
    /// Database connection for fetching initial data
    db: Option<web::Data<DatabaseConnection>>,
    /// Redis pool for caching
    redis_pool: Option<web::Data<Pool>>,
}


impl WebSocketServer {
    pub fn with_handlers(db: web::Data<DatabaseConnection>, redis_pool: web::Data<Pool>) -> Self {
        Self {
            sessions: HashMap::new(),
            user_sessions: HashMap::new(),
            subscriptions: HashMap::new(),
            subscription_params: HashMap::new(),
            session_counter: 0,
            db: Some(db),
            redis_pool: Some(redis_pool),
        }
    }

    /// Send message to all subscribed sessions for a channel
    pub fn send_to_channel(&self, channel: &SubscriptionChannel, message: WebSocketMessage) {
        if let Some(session_ids) = self.subscriptions.get(channel) {
            // Serialize message once
            if let Ok(json_msg) = serde_json::to_string(
                &crate::types::websocket::WebSocketResponse::success(message),
            ) {
                for &session_id in session_ids {
                    if let Some(addr) = self.sessions.get(&session_id) {
                        // Send pre-serialized message
                        addr.do_send(crate::types::websocket::PreSerializedMessage(
                            json_msg.clone(),
                        ));
                    }
                }
            }
        }
    }

    /// Send message to specific user sessions
    pub fn send_to_user(&self, user_id: i32, message: WebSocketMessage) {
        if let Some(session_ids) = self.user_sessions.get(&user_id) {
            // Serialize message once
            if let Ok(json_msg) = serde_json::to_string(
                &crate::types::websocket::WebSocketResponse::success(message),
            ) {
                for &session_id in session_ids {
                    if let Some(addr) = self.sessions.get(&session_id) {
                        // Send pre-serialized message
                        addr.do_send(crate::types::websocket::PreSerializedMessage(
                            json_msg.clone(),
                        ));
                    }
                }
            }
        }
    }

    /// Send message to specific session
    pub fn send_to_session(&self, session_id: usize, message: WebSocketMessage) {
        if let Some(addr) = self.sessions.get(&session_id) {
            if let Ok(json_msg) = serde_json::to_string(
                &crate::types::websocket::WebSocketResponse::success(message),
            ) {
                addr.do_send(PreSerializedMessage(json_msg));
            }
        }
    }

    /// Send message to all sessions
    #[allow(dead_code)]
    pub fn send_to_all(&self, message: WebSocketMessage) {
        if let Ok(json_msg) = serde_json::to_string(
            &crate::types::websocket::WebSocketResponse::success(message),
        ) {
            for addr in self.sessions.values() {
                addr.do_send(crate::types::websocket::PreSerializedMessage(
                    json_msg.clone(),
                ));
            }
        }
    }
}

impl Actor for WebSocketServer {
    type Context = Context<Self>;
}

/// New WebSocket session is created
#[derive(Message)]
#[rtype(usize)]
pub struct Connect {
    pub id: usize,
    pub addr: Recipient<PreSerializedMessage>,
    pub user_id: Option<i32>,
}

/// Session is disconnected
#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: usize,
}

/// Subscribe to a channel
#[derive(Message)]
#[rtype(result = "()")]
pub struct Subscribe {
    pub id: usize,
    pub channel: SubscriptionChannel,
    pub user_id: Option<i32>,
    pub params: Option<serde_json::Value>,
}

/// Unsubscribe from a channel
#[derive(Message)]
#[rtype(result = "()")]
pub struct Unsubscribe {
    pub id: usize,
    pub channel: SubscriptionChannel,
}

/// Broadcast message to channel
#[derive(Message)]
#[rtype(result = "()")]
pub struct Broadcast {
    pub channel: SubscriptionChannel,
    pub message: WebSocketMessage,
}

/// Send message to specific user
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendToUser {
    pub user_id: i32,
    pub message: WebSocketMessage,
}

/// Send message to specific session
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendToSession {
    pub session_id: usize,
    pub message: WebSocketMessage,
}

/// Broadcast events update with personalized filters for each subscriber
#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastEventsUpdate;

/// Broadcast transactions update for a specific user with personalized filters
#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastTransactionsUpdate {
    pub user_id: i32,
}

/// Broadcast portfolio update for a specific user
#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastPortfolioUpdate {
    pub user_id: i32,
}

/// Connect handler
impl Handler<Connect> for WebSocketServer {
    type Result = usize;

    fn handle(&mut self, msg: Connect, _: &mut Context<Self>) -> Self::Result {
        info!("New WebSocket connection: {}", msg.id);

        // Generate unique session id
        self.session_counter += 1;
        let session_id = self.session_counter;

        // Store session
        self.sessions.insert(session_id, msg.addr);

        // If user is authenticated, store user session mapping
        if let Some(user_id) = msg.user_id {
            self.user_sessions
                .entry(user_id)
                .or_default()
                .insert(session_id);
        }

        session_id
    }
}

/// Disconnect handler
impl Handler<Disconnect> for WebSocketServer {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) -> Self::Result {
        info!("WebSocket disconnected: {}", msg.id);

        // Remove session
        self.sessions.remove(&msg.id);

        // Remove from user sessions
        self.user_sessions.retain(|_, sessions| {
            sessions.remove(&msg.id);
            !sessions.is_empty()
        });

        // Remove from all subscriptions
        for sessions in self.subscriptions.values_mut() {
            sessions.remove(&msg.id);
        }

        // Clean up empty subscription channels
        self.subscriptions
            .retain(|_, sessions| !sessions.is_empty());

        // Remove all subscription parameters for this session
        self.subscription_params
            .retain(|(session_id, _), _| *session_id != msg.id);
    }
}

/// Subscribe handler
impl Handler<Subscribe> for WebSocketServer {
    type Result = ();

    fn handle(&mut self, msg: Subscribe, ctx: &mut Context<Self>) -> Self::Result {
        info!("Session {} subscribing to channel: {}", msg.id, msg.channel);

        // Add session to channel subscription
        self.subscriptions
            .entry(msg.channel.clone())
            .or_default()
            .insert(msg.id);

        // Store subscription parameters if provided
        if let Some(ref params) = msg.params {
            self.subscription_params
                .insert((msg.id, msg.channel.clone()), params.clone());
        }

        // Fetch and send initial data if we have database access
        if let Some(db) = &self.db {
            let db_clone = db.clone();
            let ws_server_addr = ctx.address();
            let channel = msg.channel.clone();
            let session_id = msg.id;
            let user_id = msg.user_id;
            let params = msg.params.clone();

            // Spawn async task to fetch initial data
            tokio::spawn(async move {
                let handlers =
                    crate::websocket::handlers::WebSocketHandlers::new(db_clone, ws_server_addr);

                match &channel {
                    SubscriptionChannel::Events => {
                        info!("Fetching initial events data for session {}", session_id);
                        handlers
                            .fetch_and_send_initial_events(session_id, params)
                            .await;
                    }
                    SubscriptionChannel::Event(event_id) => {
                        info!(
                            "Fetching initial event data for event {} and session {}",
                            event_id, session_id
                        );
                        handlers
                            .fetch_and_send_initial_event(session_id, *event_id)
                            .await;
                    }
                    SubscriptionChannel::Transactions => {
                        if let Some(user_id) = user_id {
                            info!(
                                "Fetching initial transactions data for user {} and session {}",
                                user_id, session_id
                            );
                            handlers
                                .fetch_and_send_initial_transactions(session_id, user_id, params)
                                .await;
                        }
                    }
                    SubscriptionChannel::Portfolio => {
                        if let Some(user_id) = user_id {
                            info!(
                                "Fetching initial portfolio data for user {} and session {}",
                                user_id, session_id
                            );
                            handlers
                                .fetch_and_send_initial_portfolio(session_id, user_id)
                                .await;
                        }
                    }
                }
            });
        }
    }
}

/// Unsubscribe handler
impl Handler<Unsubscribe> for WebSocketServer {
    type Result = ();

    fn handle(&mut self, msg: Unsubscribe, _: &mut Context<Self>) -> Self::Result {
        info!(
            "Session {} unsubscribing from channel: {}",
            msg.id, msg.channel
        );

        // Remove session from channel subscription
        if let Some(sessions) = self.subscriptions.get_mut(&msg.channel) {
            sessions.remove(&msg.id);

            // Clean up empty subscription channels
            if sessions.is_empty() {
                self.subscriptions.remove(&msg.channel);
            }
        }

        // Remove subscription parameters
        self.subscription_params
            .remove(&(msg.id, msg.channel.clone()));
    }
}

/// Broadcast handler
impl Handler<Broadcast> for WebSocketServer {
    type Result = ();

    fn handle(&mut self, msg: Broadcast, _: &mut Context<Self>) -> Self::Result {
        self.send_to_channel(&msg.channel, msg.message);
    }
}

/// Send to user handler
impl Handler<SendToUser> for WebSocketServer {
    type Result = ();

    fn handle(&mut self, msg: SendToUser, _: &mut Context<Self>) -> Self::Result {
        self.send_to_user(msg.user_id, msg.message);
    }
}

/// Send to session handler
impl Handler<SendToSession> for WebSocketServer {
    type Result = ();

    fn handle(&mut self, msg: SendToSession, _: &mut Context<Self>) -> Self::Result {
        self.send_to_session(msg.session_id, msg.message);
    }
}

/// Broadcast events update handler - sends personalized data to each subscriber
impl Handler<BroadcastEventsUpdate> for WebSocketServer {
    type Result = ();

    fn handle(&mut self, _msg: BroadcastEventsUpdate, ctx: &mut Context<Self>) -> Self::Result {
        // Get all sessions subscribed to events channel
        if let Some(session_ids) = self.subscriptions.get(&SubscriptionChannel::Events) {
            if let (Some(db), Some(_redis_pool)) = (&self.db, &self.redis_pool) {
                for &session_id in session_ids {
                    // Get stored parameters for this session
                    let params = self
                        .subscription_params
                        .get(&(session_id, SubscriptionChannel::Events))
                        .cloned();

                    let db_clone = db.clone();
                    let ws_server_addr = ctx.address();

                    // Spawn async task to fetch and send personalized data
                    tokio::spawn(async move {
                        let handlers = crate::websocket::handlers::WebSocketHandlers::new(
                            db_clone,
                            ws_server_addr,
                        );

                        // Fetch and send events with the session's specific parameters
                        handlers
                            .fetch_and_send_initial_events(session_id, params)
                            .await;
                    });
                }
            }
        }
    }
}

/// Broadcast transactions update handler - sends personalized data to each subscriber
impl Handler<BroadcastTransactionsUpdate> for WebSocketServer {
    type Result = ();

    fn handle(
        &mut self,
        msg: BroadcastTransactionsUpdate,
        ctx: &mut Context<Self>,
    ) -> Self::Result {
        // Get all sessions for this user subscribed to transactions
        if let Some(user_sessions) = self.user_sessions.get(&msg.user_id) {
            for &session_id in user_sessions {
                // Check if this session is subscribed to transactions
                if let Some(sessions) = self.subscriptions.get(&SubscriptionChannel::Transactions) {
                    if sessions.contains(&session_id) {
                        // Get stored parameters for this session
                        let params = self
                            .subscription_params
                            .get(&(session_id, SubscriptionChannel::Transactions))
                            .cloned();

                        if let (Some(db), Some(_redis_pool)) = (&self.db, &self.redis_pool) {
                            let db_clone = db.clone();
                            let ws_server_addr = ctx.address();
                            let user_id = msg.user_id;

                            tokio::spawn(async move {
                                let handlers = crate::websocket::handlers::WebSocketHandlers::new(
                                    db_clone,
                                    ws_server_addr,
                                );

                                handlers
                                    .fetch_and_send_initial_transactions(
                                        session_id, user_id, params,
                                    )
                                    .await;
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Broadcast portfolio update handler - sends personalized portfolio data to each subscriber
impl Handler<BroadcastPortfolioUpdate> for WebSocketServer {
    type Result = ();

    fn handle(&mut self, msg: BroadcastPortfolioUpdate, ctx: &mut Context<Self>) -> Self::Result {
        // Get all sessions for this user subscribed to portfolio
        if let Some(user_sessions) = self.user_sessions.get(&msg.user_id) {
            for &session_id in user_sessions {
                // Check if this session is subscribed to portfolio
                if let Some(sessions) = self.subscriptions.get(&SubscriptionChannel::Portfolio) {
                    if sessions.contains(&session_id) {
                        if let (Some(db), Some(_redis_pool)) = (&self.db, &self.redis_pool) {
                            let db_clone = db.clone();
                            let ws_server_addr = ctx.address();
                            let user_id = msg.user_id;

                            tokio::spawn(async move {
                                let handlers = crate::websocket::handlers::WebSocketHandlers::new(
                                    db_clone,
                                    ws_server_addr,
                                );

                                handlers.fetch_and_broadcast_portfolio(user_id).await;
                            });
                        }
                    }
                }
            }
        }
    }
}
