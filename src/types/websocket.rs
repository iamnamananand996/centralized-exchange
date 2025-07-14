use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use actix::Message;
use crate::types::{
    event::EventResponse,
    transaction::TransactionResponse,
};
use crate::utils::pagination::PaginatedResponse;

/// Pre-serialized message for efficient broadcasting
pub struct PreSerializedMessage(pub String);

impl Message for PreSerializedMessage {
    type Result = ();
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum WebSocketMessage {
    #[serde(rename = "events_data")]
    EventsData {
        data: PaginatedResponse<EventResponse>,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "event_data")]
    EventData {
        event: EventResponse,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "transactions_data")]
    TransactionsData {
        data: PaginatedResponse<TransactionResponse>,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "portfolio_data")]
    PortfolioData {
        data: serde_json::Value, // Will be replaced with position-based portfolio
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "subscribe")]
    Subscribe {
        channel: String,
        params: Option<serde_json::Value>,
    },
    #[serde(rename = "unsubscribe")]
    Unsubscribe {
        channel: String,
    },
    #[serde(rename = "ping")]
    Ping {
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "pong")]
    Pong {
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketRequest {
    pub r#type: String,
    pub channel: Option<String>,
    pub params: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct WebSocketResponse {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<WebSocketMessage>,
}

impl WebSocketResponse {
    pub fn success(data: WebSocketMessage) -> Self {
        Self {
            success: true,
            message: None,
            data: Some(data),
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            message: Some(message),
            data: None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum SubscriptionChannel {
    Events,
    Event(i32),
    Transactions,
    Portfolio,
}

impl std::fmt::Display for SubscriptionChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubscriptionChannel::Events => write!(f, "events"),
            SubscriptionChannel::Event(id) => write!(f, "event:{}", id),
            SubscriptionChannel::Transactions => write!(f, "transactions"),
            SubscriptionChannel::Portfolio => write!(f, "portfolio"),
        }
    }
}

impl SubscriptionChannel {
    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "events" => Some(SubscriptionChannel::Events),
            "transactions" => Some(SubscriptionChannel::Transactions),
            "portfolio" => Some(SubscriptionChannel::Portfolio),
            _ => {
                if s.starts_with("event:") {
                    let id_str = &s[6..];
                    id_str.parse::<i32>().ok().map(SubscriptionChannel::Event)
                } else {
                    None
                }
            }
        }
    }
}

impl Message for WebSocketMessage {
    type Result = ();
} 