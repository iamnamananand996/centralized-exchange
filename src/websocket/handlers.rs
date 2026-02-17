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
    event::{EventResponse, ListEventsQuery},
    transaction::TransactionResponse,
};
use crate::utils::pagination::{PaginatedResponse, PaginationInfo, PaginationQuery};
use crate::websocket::server::{Broadcast, SendToUser, WebSocketServer};
use entity::{event_options, events, transaction, users};
use sea_orm::prelude::Decimal;

pub struct WebSocketHandlers {
    pub db: web::Data<DatabaseConnection>,
    pub ws_server: Addr<WebSocketServer>,
}

impl WebSocketHandlers {
    pub fn new(db: web::Data<DatabaseConnection>, ws_server: Addr<WebSocketServer>) -> Self {
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

    /// Fetch and broadcast portfolio data based on positions
    pub async fn fetch_and_broadcast_portfolio(&self, user_id: i32) {
        // Import position tracker
        use crate::order_book::position_tracker::PositionTracker;
        

        // Get user data
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

        let position_tracker = PositionTracker::new(self.db.get_ref().clone());

        // Get all user positions
        let _positions = match position_tracker.get_user_positions(user_id).await {
            Ok(pos) => pos,
            Err(e) => {
                error!("Failed to get user positions: {}", e);
                return;
            }
        };

        // Group positions by event
        let grouped_positions = match position_tracker.get_portfolio_positions(user_id).await {
            Ok(grouped) => grouped,
            Err(e) => {
                error!("Failed to get portfolio positions: {}", e);
                return;
            }
        };

        let mut total_invested = Decimal::new(0, 2);
        let mut current_value = Decimal::new(0, 2);
        let _portfolio_data = serde_json::json!({
            "active_positions": []
        });

        let mut active_positions = Vec::new();

        // Process each event group
        for (event_id, event_positions) in grouped_positions {
            // Get event details
            let event = match events::Entity::find_by_id(event_id)
                .one(self.db.get_ref())
                .await
            {
                Ok(Some(e)) => e,
                _ => continue,
            };

            let mut event_invested = Decimal::new(0, 2);
            let mut event_current_value = Decimal::new(0, 2);
            let mut position_details = Vec::new();

            // Process each position in the event
            for position in event_positions {
                // Get option details
                let option = match event_options::Entity::find_by_id(position.option_id)
                    .one(self.db.get_ref())
                    .await
                {
                    Ok(Some(o)) => o,
                    _ => continue,
                };

                let position_cost = position.average_price * Decimal::from(position.quantity);
                let position_value = option.current_price * Decimal::from(position.quantity);

                event_invested += position_cost;
                event_current_value += position_value;

                position_details.push(serde_json::json!({
                    "option_id": position.option_id,
                    "option_text": option.option_text,
                    "quantity": position.quantity,
                    "avg_price": position.average_price,
                    "current_price": option.current_price,
                    "position_value": position_value,
                }));
            }

            let event_pnl = event_current_value - event_invested;

            total_invested += event_invested;
            current_value += event_current_value;

            active_positions.push(serde_json::json!({
                "event_id": event.id,
                "event_title": event.title,
                "event_status": event.status,
                "invested": event_invested,
                "current_value": event_current_value,
                "pnl": event_pnl,
                "positions": position_details,
            }));
        }

        let total_pnl = current_value - total_invested;

        let portfolio_data = serde_json::json!({
            "total_invested": total_invested,
            "current_value": current_value,
            "total_pnl": total_pnl,
            "wallet_balance": user.wallet_balance,
            "active_positions": active_positions,
        });

        let message = WebSocketMessage::PortfolioData {
            data: portfolio_data,
            timestamp: Utc::now(),
        };

        self.ws_server.do_send(SendToUser { user_id, message });
    }

    /// Fetch and send initial portfolio data to a specific session
    pub async fn fetch_and_send_initial_portfolio(&self, session_id: usize, user_id: i32) {
        // Import position tracker
        use crate::order_book::position_tracker::PositionTracker;

        // Get user data
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

        let position_tracker = PositionTracker::new(self.db.get_ref().clone());

        // Get grouped positions
        let grouped_positions = match position_tracker.get_portfolio_positions(user_id).await {
            Ok(grouped) => grouped,
            Err(e) => {
                error!("Failed to get portfolio positions: {}", e);
                return;
            }
        };

        let mut total_invested = Decimal::new(0, 2);
        let mut current_value = Decimal::new(0, 2);
        let mut active_positions = Vec::new();

        // Process each event group
        for (event_id, event_positions) in grouped_positions {
            // Get event details
            let event = match events::Entity::find_by_id(event_id)
                .one(self.db.get_ref())
                .await
            {
                Ok(Some(e)) => e,
                _ => continue,
            };

            let mut event_invested = Decimal::new(0, 2);
            let mut event_current_value = Decimal::new(0, 2);
            let mut position_details = Vec::new();

            // Process each position in the event
            for position in event_positions {
                // Get option details
                let option = match event_options::Entity::find_by_id(position.option_id)
                    .one(self.db.get_ref())
                    .await
                {
                    Ok(Some(o)) => o,
                    _ => continue,
                };

                let position_cost = position.average_price * Decimal::from(position.quantity);
                let position_value = option.current_price * Decimal::from(position.quantity);

                event_invested += position_cost;
                event_current_value += position_value;

                position_details.push(serde_json::json!({
                    "option_id": position.option_id,
                    "option_text": option.option_text,
                    "quantity": position.quantity,
                    "avg_price": position.average_price,
                    "current_price": option.current_price,
                    "position_value": position_value,
                }));
            }

            let event_pnl = event_current_value - event_invested;

            total_invested += event_invested;
            current_value += event_current_value;

            active_positions.push(serde_json::json!({
                "event_id": event.id,
                "event_title": event.title,
                "event_status": event.status,
                "invested": event_invested,
                "current_value": event_current_value,
                "pnl": event_pnl,
                "positions": position_details,
            }));
        }

        let total_pnl = current_value - total_invested;

        let portfolio_data = serde_json::json!({
            "total_invested": total_invested,
            "current_value": current_value,
            "total_pnl": total_pnl,
            "wallet_balance": user.wallet_balance,
            "active_positions": active_positions,
        });

        let message = WebSocketMessage::PortfolioData {
            data: portfolio_data,
            timestamp: Utc::now(),
        };

        self.ws_server
            .do_send(crate::websocket::server::SendToSession {
                session_id,
                message,
            });
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
