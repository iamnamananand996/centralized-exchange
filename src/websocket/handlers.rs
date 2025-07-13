use actix::Addr;
use actix_web::web;
use chrono::Utc;
use log::{error, info, warn};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde_json;

use crate::types::websocket::{SubscriptionChannel, WebSocketMessage};
use crate::types::{
    bet::{
        ActivePosition, EventSummary, MyBetResponse, MyBetsQuery, OptionSummary, PortfolioResponse,
        PositionDetail,
    },
    event::{EventResponse, ListEventsQuery},
    transaction::TransactionResponse,
};
use crate::utils::pagination::{PaginatedResponse, PaginationInfo, PaginationQuery};
use crate::websocket::server::{Broadcast, SendToUser, WebSocketServer};
use entity::{bets, event_options, events, transaction, users};
use sea_orm::prelude::Decimal;

pub struct WebSocketHandlers {
    pub db: web::Data<DatabaseConnection>,
    pub ws_server: Addr<WebSocketServer>,
}

impl WebSocketHandlers {
    pub fn new(
        db: web::Data<DatabaseConnection>,
        ws_server: Addr<WebSocketServer>,
    ) -> Self {
        Self { db, ws_server }
    }

    /// Fetch and broadcast single event data
    pub async fn fetch_and_broadcast_event(&self, event_id: i32) {
        let event = match events::Entity::find_by_id(event_id)
            .one(self.db.get_ref())
            .await
        {
            Ok(Some(event)) => event,
            Ok(None) => {
                warn!("Event not found: {}", event_id);
                return;
            }
            Err(e) => {
                error!("Failed to fetch event: {}", e);
                return;
            }
        };

        let options = match event_options::Entity::find()
            .filter(event_options::Column::EventId.eq(event_id))
            .all(self.db.get_ref())
            .await
        {
            Ok(options) => options,
            Err(e) => {
                error!("Failed to fetch event options: {}", e);
                return;
            }
        };

        let event_response = EventResponse::from((event, options));

        let message = WebSocketMessage::EventData {
            event: event_response,
            timestamp: Utc::now(),
        };

        self.ws_server.do_send(Broadcast {
            channel: SubscriptionChannel::Event(event_id),
            message,
        });
    }

    /// Fetch and broadcast portfolio data for a user
    pub async fn fetch_and_broadcast_portfolio(&self, user_id: i32) {
        // Get user's current balance
        let user = match users::Entity::find_by_id(user_id)
            .one(self.db.get_ref())
            .await
        {
            Ok(Some(user)) => user,
            Ok(None) => {
                warn!("User not found: {}", user_id);
                return;
            }
            Err(e) => {
                error!("Failed to fetch user: {}", e);
                return;
            }
        };

        // Get all active bets with related data
        let active_bets = match bets::Entity::find()
            .filter(bets::Column::UserId.eq(user_id))
            .filter(bets::Column::Status.eq("active"))
            .find_also_related(events::Entity)
            .find_also_related(event_options::Entity)
            .all(self.db.get_ref())
            .await
        {
            Ok(bets) => bets,
            Err(e) => {
                error!("Failed to fetch active bets: {}", e);
                return;
            }
        };

        let mut total_invested = Decimal::new(0, 2);
        let mut current_value = Decimal::new(0, 2);
        let mut positions_map: std::collections::HashMap<i32, ActivePosition> =
            std::collections::HashMap::new();

        for (bet, event_opt, option_opt) in active_bets {
            let event = event_opt.unwrap_or_else(|| events::Model {
                id: 0,
                title: "Unknown Event".to_string(),
                status: "unknown".to_string(),
                description: "".to_string(),
                category: "".to_string(),
                end_time: chrono::NaiveDateTime::default(),
                min_bet_amount: Decimal::new(0, 2),
                max_bet_amount: Decimal::new(0, 2),
                total_volume: Decimal::new(0, 2),
                image_url: "".to_string(),
                created_by: 0,
                resolved_by: 0,
                winning_option_id: 0,
                resolution_note: "".to_string(),
                resolved_at: chrono::NaiveDateTime::default(),
                created_at: chrono::NaiveDateTime::default(),
                updated_at: chrono::NaiveDateTime::default(),
            });

            let option = option_opt.unwrap_or_else(|| event_options::Model {
                id: 0,
                event_id: 0,
                option_text: "Unknown Option".to_string(),
                current_price: Decimal::new(0, 2),
                total_backing: Decimal::new(0, 2),
                is_winning_option: None,
            });

            let bet_current_value = option.current_price * Decimal::from(bet.quantity);
            total_invested += bet.total_amount;
            current_value += bet_current_value;

            let position = positions_map
                .entry(event.id)
                .or_insert_with(|| ActivePosition {
                    event_id: event.id,
                    event_title: event.title.clone(),
                    invested: Decimal::new(0, 2),
                    current_value: Decimal::new(0, 2),
                    pnl: Decimal::new(0, 2),
                    positions: Vec::new(),
                });

            position.invested += bet.total_amount;
            position.current_value += bet_current_value;
            position.pnl = position.current_value - position.invested;

            position.positions.push(PositionDetail {
                option_text: option.option_text,
                quantity: bet.quantity,
                avg_price: bet.price_per_share,
                current_price: option.current_price,
            });
        }

        let total_pnl = current_value - total_invested;
        let active_positions: Vec<ActivePosition> = positions_map.into_values().collect();

        let portfolio = PortfolioResponse {
            total_invested,
            current_value,
            total_pnl,
            wallet_balance: user.wallet_balance,
            active_positions,
        };

        let message = WebSocketMessage::PortfolioData {
            data: portfolio,
            timestamp: Utc::now(),
        };

        self.ws_server.do_send(SendToUser { user_id, message });
    }

    /// Fetch and send initial events data to a specific session
    pub async fn fetch_and_send_initial_events(
        &self,
        session_id: usize,
        query_params: Option<serde_json::Value>,
    ) {
        // Debug log the raw params
        info!("Raw query_params received: {:?}", query_params);

        // Parse query parameters with better error handling
        let query = if let Some(params) = query_params {
            match serde_json::from_value::<ListEventsQuery>(params.clone()) {
                Ok(q) => q,
                Err(e) => {
                    warn!(
                        "Failed to parse events query params: {}. Params: {:?}",
                        e, params
                    );
                    // If the params don't match the expected flattened structure,
                    // try to handle common patterns
                    if let Some(obj) = params.as_object() {
                        // Create a flattened version for ListEventsQuery
                        let mut flattened = serde_json::Map::new();

                        // Copy over direct fields
                        if let Some(status) = obj.get("status") {
                            flattened.insert("status".to_string(), status.clone());
                        }
                        if let Some(category) = obj.get("category") {
                            flattened.insert("category".to_string(), category.clone());
                        }

                        // Convert pagination fields to strings if they're numbers
                        if let Some(page_val) = obj.get("page") {
                            let page_str = match page_val {
                                serde_json::Value::Number(n) => {
                                    serde_json::Value::String(n.to_string())
                                }
                                serde_json::Value::String(_) => page_val.clone(),
                                _ => serde_json::Value::String("1".to_string()),
                            };
                            flattened.insert("page".to_string(), page_str);
                        }
                        if let Some(limit_val) = obj.get("limit") {
                            let limit_str = match limit_val {
                                serde_json::Value::Number(n) => {
                                    serde_json::Value::String(n.to_string())
                                }
                                serde_json::Value::String(_) => limit_val.clone(),
                                _ => serde_json::Value::String("10".to_string()),
                            };
                            flattened.insert("limit".to_string(), limit_str);
                        }

                        // Try to deserialize the flattened version
                        match serde_json::from_value::<ListEventsQuery>(serde_json::Value::Object(
                            flattened,
                        )) {
                            Ok(q) => q,
                            Err(e) => {
                                warn!("Failed to parse flattened params: {}", e);
                                ListEventsQuery::default()
                            }
                        }
                    } else {
                        ListEventsQuery::default()
                    }
                }
            }
        } else {
            ListEventsQuery::default()
        };

        // Debug logging to see what we parsed
        info!(
            "Parsed events query - status: {:?}, category: {:?}, page: {}, limit: {}",
            query.status,
            query.category,
            query.pagination.get_page(),
            query.pagination.get_limit()
        );

        let mut events_query = events::Entity::find();

        // Apply filters
        if let Some(status) = &query.status {
            events_query = events_query.filter(events::Column::Status.eq(status));
        }
        if let Some(category) = &query.category {
            events_query = events_query.filter(events::Column::Category.eq(category));
        }

        // Apply pagination
        let page = query.pagination.get_page();
        let limit = query.pagination.get_limit();
        let offset = query.pagination.get_offset();

        // Get total count
        let total_count = match events_query.to_owned().count(self.db.get_ref()).await {
            Ok(count) => count,
            Err(e) => {
                error!("Failed to get events count: {}", e);
                return;
            }
        };

        // Get events
        let events = match events_query
            .order_by_desc(events::Column::CreatedAt)
            .limit(limit)
            .offset(offset)
            .all(self.db.get_ref())
            .await
        {
            Ok(events) => events,
            Err(e) => {
                error!("Failed to fetch events: {}", e);
                return;
            }
        };

        // Fetch options for all events
        let mut events_response: Vec<EventResponse> = Vec::new();
        for event in events {
            let options = match event_options::Entity::find()
                .filter(event_options::Column::EventId.eq(event.id))
                .all(self.db.get_ref())
                .await
            {
                Ok(options) => options,
                Err(e) => {
                    error!("Failed to fetch event options: {}", e);
                    continue;
                }
            };

            events_response.push(EventResponse::from((event, options)));
        }

        let pagination_info = PaginationInfo::new(page, total_count, limit);
        let response = PaginatedResponse::new(events_response, pagination_info);

        let message = WebSocketMessage::EventsData {
            data: response,
            timestamp: Utc::now(),
        };

        self.ws_server
            .do_send(crate::websocket::server::SendToSession {
                session_id,
                message,
            });
    }

    /// Fetch and send initial event data to a specific session
    pub async fn fetch_and_send_initial_event(&self, session_id: usize, event_id: i32) {
        let event = match events::Entity::find_by_id(event_id)
            .one(self.db.get_ref())
            .await
        {
            Ok(Some(event)) => event,
            Ok(None) => {
                warn!("Event not found: {}", event_id);
                return;
            }
            Err(e) => {
                error!("Failed to fetch event: {}", e);
                return;
            }
        };

        let options = match event_options::Entity::find()
            .filter(event_options::Column::EventId.eq(event_id))
            .all(self.db.get_ref())
            .await
        {
            Ok(options) => options,
            Err(e) => {
                error!("Failed to fetch event options: {}", e);
                return;
            }
        };

        let event_response = EventResponse::from((event, options));

        let message = WebSocketMessage::EventData {
            event: event_response,
            timestamp: Utc::now(),
        };

        self.ws_server
            .do_send(crate::websocket::server::SendToSession {
                session_id,
                message,
            });
    }

    /// Fetch and send initial transactions data to a specific session
    pub async fn fetch_and_send_initial_transactions(
        &self,
        session_id: usize,
        user_id: i32,
        query_params: Option<serde_json::Value>,
    ) {
        // Parse query parameters with better error handling
        let query = if let Some(params) = query_params {
            match serde_json::from_value::<PaginationQuery>(params.clone()) {
                Ok(q) => q,
                Err(e) => {
                    warn!(
                        "Failed to parse transactions query params: {}. Params: {:?}",
                        e, params
                    );
                    // Handle common patterns
                    if let Some(obj) = params.as_object() {
                        let mut flattened = serde_json::Map::new();

                        // Convert pagination fields to strings if they're numbers
                        if let Some(page_val) = obj.get("page") {
                            let page_str = match page_val {
                                serde_json::Value::Number(n) => {
                                    serde_json::Value::String(n.to_string())
                                }
                                serde_json::Value::String(_) => page_val.clone(),
                                _ => serde_json::Value::String("1".to_string()),
                            };
                            flattened.insert("page".to_string(), page_str);
                        }
                        if let Some(limit_val) = obj.get("limit") {
                            let limit_str = match limit_val {
                                serde_json::Value::Number(n) => {
                                    serde_json::Value::String(n.to_string())
                                }
                                serde_json::Value::String(_) => limit_val.clone(),
                                _ => serde_json::Value::String("10".to_string()),
                            };
                            flattened.insert("limit".to_string(), limit_str);
                        }

                        match serde_json::from_value::<PaginationQuery>(serde_json::Value::Object(
                            flattened,
                        )) {
                            Ok(q) => q,
                            Err(e) => {
                                warn!("Failed to parse flattened transactions params: {}", e);
                                PaginationQuery::default()
                            }
                        }
                    } else {
                        PaginationQuery::default()
                    }
                }
            }
        } else {
            PaginationQuery::default()
        };

        let page = query.get_page();
        let limit = query.get_limit();
        let offset = query.get_offset();

        // Get total count
        let total_count = match transaction::Entity::find()
            .filter(transaction::Column::UserId.eq(user_id))
            .count(self.db.get_ref())
            .await
        {
            Ok(count) => count,
            Err(e) => {
                error!("Failed to get transactions count: {}", e);
                return;
            }
        };

        // Get transactions
        let transactions = match transaction::Entity::find()
            .filter(transaction::Column::UserId.eq(user_id))
            .order_by_desc(transaction::Column::CreatedAt)
            .offset(offset)
            .limit(limit)
            .all(self.db.get_ref())
            .await
        {
            Ok(transactions) => transactions,
            Err(e) => {
                error!("Failed to fetch transactions: {}", e);
                return;
            }
        };

        let transactions_response: Vec<TransactionResponse> = transactions
            .into_iter()
            .map(|t| TransactionResponse {
                id: t.id,
                user_id: t.user_id,
                r#type: t.r#type,
                amount: t.amount.to_string().parse::<f64>().unwrap_or(0.0),
                balance_before: t.balance_before.to_string().parse::<f64>().unwrap_or(0.0),
                balance_after: t.balance_after.to_string().parse::<f64>().unwrap_or(0.0),
                status: t.status,
                reference_id: t.reference_id,
                created_at: t.created_at.and_utc(),
            })
            .collect();

        let pagination_info = PaginationInfo::new(page, total_count, limit);
        let response = PaginatedResponse::new(transactions_response, pagination_info);

        let message = WebSocketMessage::TransactionsData {
            data: response,
            timestamp: Utc::now(),
        };

        self.ws_server
            .do_send(crate::websocket::server::SendToSession {
                session_id,
                message,
            });
    }

    /// Fetch and send initial my bets data to a specific session
    pub async fn fetch_and_send_initial_my_bets(
        &self,
        session_id: usize,
        user_id: i32,
        query_params: Option<serde_json::Value>,
    ) {
        // Parse query parameters with better error handling
        let query = if let Some(params) = query_params {
            match serde_json::from_value::<MyBetsQuery>(params.clone()) {
                Ok(q) => q,
                Err(e) => {
                    warn!(
                        "Failed to parse my_bets query params: {}. Params: {:?}",
                        e, params
                    );
                    // Handle common patterns
                    if let Some(obj) = params.as_object() {
                        let mut flattened = serde_json::Map::new();

                        // Copy over direct fields
                        if let Some(status) = obj.get("status") {
                            flattened.insert("status".to_string(), status.clone());
                        }

                        // Convert pagination fields to strings if they're numbers
                        if let Some(page_val) = obj.get("page") {
                            let page_str = match page_val {
                                serde_json::Value::Number(n) => {
                                    serde_json::Value::String(n.to_string())
                                }
                                serde_json::Value::String(_) => page_val.clone(),
                                _ => serde_json::Value::String("1".to_string()),
                            };
                            flattened.insert("page".to_string(), page_str);
                        }
                        if let Some(limit_val) = obj.get("limit") {
                            let limit_str = match limit_val {
                                serde_json::Value::Number(n) => {
                                    serde_json::Value::String(n.to_string())
                                }
                                serde_json::Value::String(_) => limit_val.clone(),
                                _ => serde_json::Value::String("10".to_string()),
                            };
                            flattened.insert("limit".to_string(), limit_str);
                        }

                        match serde_json::from_value::<MyBetsQuery>(serde_json::Value::Object(
                            flattened,
                        )) {
                            Ok(q) => q,
                            Err(e) => {
                                warn!("Failed to parse flattened my_bets params: {}", e);
                                MyBetsQuery::default()
                            }
                        }
                    } else {
                        MyBetsQuery::default()
                    }
                }
            }
        } else {
            MyBetsQuery::default()
        };

        // Apply pagination
        let page = query.pagination.get_page();
        let limit = query.pagination.get_limit();
        let offset = query.pagination.get_offset();

        // Get total count first (before adding related entities)
        let mut count_query = bets::Entity::find().filter(bets::Column::UserId.eq(user_id));

        if let Some(status) = &query.status {
            count_query = count_query.filter(bets::Column::Status.eq(status));
        }

        let total_count = match count_query.count(self.db.get_ref()).await {
            Ok(count) => count,
            Err(e) => {
                error!("Failed to get bets count: {}", e);
                return;
            }
        };

        // Now build the query with related data
        let mut bets_query = bets::Entity::find()
            .filter(bets::Column::UserId.eq(user_id))
            .find_also_related(events::Entity)
            .find_also_related(event_options::Entity);

        // Apply status filter
        if let Some(status) = &query.status {
            bets_query = bets_query.filter(bets::Column::Status.eq(status));
        }

        // Get bets with related data (similar logic as fetch_and_broadcast_my_bets)
        let bets_result = match bets_query
            .order_by_desc(bets::Column::PlacedAt)
            .offset(offset)
            .limit(limit)
            .all(self.db.get_ref())
            .await
        {
            Ok(bets) => bets,
            Err(e) => {
                error!("Failed to fetch bets: {}", e);
                return;
            }
        };

        let mut my_bets: Vec<MyBetResponse> = Vec::new();
        for (bet, event_opt, option_opt) in bets_result {
            let (event, option) = match (event_opt, option_opt) {
                (Some(e), Some(o)) => (e, o),
                _ => {
                    warn!("Missing event or option data for bet {}", bet.id);
                    continue;
                }
            };

            let bet_current_value = option.current_price * Decimal::from(bet.quantity);
            let bet_total_cost = bet.price_per_share * Decimal::from(bet.quantity);
            let pnl = bet_current_value - bet_total_cost;

            my_bets.push(MyBetResponse {
                id: bet.id,
                event: EventSummary {
                    id: event.id,
                    title: event.title,
                    status: event.status,
                },
                option: OptionSummary {
                    id: option.id,
                    option_text: option.option_text,
                    current_price: option.current_price,
                },
                quantity: bet.quantity,
                price_per_share: bet.price_per_share,
                total_amount: bet.total_amount,
                current_value: bet_current_value,
                pnl,
                status: bet.status,
                placed_at: bet.placed_at.and_utc(),
            });
        }

        let pagination_info = PaginationInfo::new(page, total_count, limit);
        let response = PaginatedResponse::new(my_bets, pagination_info);

        let message = WebSocketMessage::MyBetsData {
            data: response,
            timestamp: Utc::now(),
        };

        self.ws_server
            .do_send(crate::websocket::server::SendToSession {
                session_id,
                message,
            });
    }

    /// Fetch and send initial portfolio data to a specific session
    pub async fn fetch_and_send_initial_portfolio(&self, session_id: usize, user_id: i32) {
        // Similar logic as fetch_and_broadcast_portfolio but send to specific session
        // Get user's current balance
        let user = match users::Entity::find_by_id(user_id)
            .one(self.db.get_ref())
            .await
        {
            Ok(Some(user)) => user,
            Ok(None) => {
                warn!("User not found: {}", user_id);
                return;
            }
            Err(e) => {
                error!("Failed to fetch user: {}", e);
                return;
            }
        };

        // Get all active bets with related data
        let active_bets = match bets::Entity::find()
            .filter(bets::Column::UserId.eq(user_id))
            .filter(bets::Column::Status.eq("active"))
            .find_also_related(events::Entity)
            .find_also_related(event_options::Entity)
            .all(self.db.get_ref())
            .await
        {
            Ok(bets) => bets,
            Err(e) => {
                error!("Failed to fetch active bets: {}", e);
                return;
            }
        };

        let mut total_invested = Decimal::new(0, 2);
        let mut current_value = Decimal::new(0, 2);
        let mut positions_map: std::collections::HashMap<i32, ActivePosition> =
            std::collections::HashMap::new();

        for (bet, event_opt, option_opt) in active_bets {
            let (event, option) = match (event_opt, option_opt) {
                (Some(e), Some(o)) => (e, o),
                _ => continue,
            };

            let bet_current_value = option.current_price * Decimal::from(bet.quantity);
            let bet_total_cost = bet.price_per_share * Decimal::from(bet.quantity);

            total_invested += bet_total_cost;
            current_value += bet_current_value;

            // Group by event
            let position = positions_map
                .entry(event.id)
                .or_insert_with(|| ActivePosition {
                    event_id: event.id,
                    event_title: event.title.clone(),
                    invested: Decimal::new(0, 2),
                    current_value: Decimal::new(0, 2),
                    pnl: Decimal::new(0, 2),
                    positions: Vec::new(),
                });

            position.invested += bet_total_cost;
            position.current_value += bet_current_value;
            position.pnl = position.current_value - position.invested;

            position.positions.push(PositionDetail {
                option_text: option.option_text,
                quantity: bet.quantity,
                avg_price: bet.price_per_share,
                current_price: option.current_price,
            });
        }

        let total_pnl = current_value - total_invested;
        let active_positions: Vec<ActivePosition> = positions_map.into_values().collect();

        let portfolio = PortfolioResponse {
            total_invested,
            current_value,
            total_pnl,
            wallet_balance: user.wallet_balance,
            active_positions,
        };

        let message = WebSocketMessage::PortfolioData {
            data: portfolio,
            timestamp: Utc::now(),
        };

        self.ws_server
            .do_send(crate::websocket::server::SendToSession {
                session_id,
                message,
            });
    }
}

// Default implementations for query types
impl Default for ListEventsQuery {
    fn default() -> Self {
        Self {
            status: None,
            category: None,
            pagination: PaginationQuery::default(),
        }
    }
}

impl Default for MyBetsQuery {
    fn default() -> Self {
        Self {
            status: None,
            pagination: PaginationQuery::default(),
        }
    }
}

impl Default for PaginationQuery {
    fn default() -> Self {
        Self {
            page: Some(1),
            limit: Some(10),
        }
    }
}
