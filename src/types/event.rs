use crate::utils::pagination::PaginationQuery;
use chrono::{DateTime, Utc};
use entity::{event_options, events};
use sea_orm::prelude::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct CreateEventRequest {
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub end_time: DateTime<Utc>,
    pub min_bet_amount: Option<Decimal>,
    pub max_bet_amount: Option<Decimal>,
    pub image_url: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateEventRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub end_time: Option<DateTime<Utc>>,
    pub min_bet_amount: Option<Decimal>,
    pub max_bet_amount: Option<Decimal>,
    pub image_url: Option<String>,
}

#[derive(Default, Deserialize)]
pub struct ListEventsQuery {
    pub status: Option<String>,
    pub category: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Serialize)]
pub struct OptionResponse {
    pub id: i32,
    pub option_text: String,
    pub current_price: Decimal,
    pub total_backing: Decimal,
    pub is_winning_option: Option<bool>,
}

impl From<event_options::Model> for OptionResponse {
    fn from(option: event_options::Model) -> Self {
        Self {
            id: option.id,
            option_text: option.option_text,
            current_price: option.current_price,
            total_backing: option.total_backing,
            is_winning_option: option.is_winning_option,
        }
    }
}

#[derive(Serialize)]
pub struct EventResponse {
    pub id: i32,
    pub title: String,
    pub description: String,
    pub category: String,
    pub status: String,
    pub end_time: chrono::NaiveDateTime,
    pub min_bet_amount: Decimal,
    pub max_bet_amount: Decimal,
    pub total_volume: Decimal,
    pub image_url: String,
    pub created_by: i32,
    pub resolved_by: Option<i32>,
    pub winning_option_id: Option<i32>,
    pub resolution_note: String,
    pub resolved_at: Option<chrono::NaiveDateTime>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub options: Vec<OptionResponse>,
}

impl From<(events::Model, Vec<event_options::Model>)> for EventResponse {
    fn from((event, options): (events::Model, Vec<event_options::Model>)) -> Self {
        Self {
            id: event.id,
            title: event.title,
            description: event.description,
            category: event.category,
            status: event.status,
            end_time: event.end_time,
            min_bet_amount: event.min_bet_amount,
            max_bet_amount: event.max_bet_amount,
            total_volume: event.total_volume,
            image_url: event.image_url,
            created_by: event.created_by,
            resolved_by: if event.resolved_by == 0 {
                None
            } else {
                Some(event.resolved_by)
            },
            winning_option_id: if event.winning_option_id == 0 {
                None
            } else {
                Some(event.winning_option_id)
            },
            resolution_note: event.resolution_note,
            resolved_at: if event.resolved_at == chrono::NaiveDateTime::default() {
                None
            } else {
                Some(event.resolved_at)
            },
            created_at: event.created_at,
            updated_at: event.updated_at,
            options: options.into_iter().map(OptionResponse::from).collect(),
        }
    }
}

#[derive(Deserialize)]
pub struct SettleEventRequest {
    pub winning_option_id: i32,
    pub resolution_note: Option<String>,
}

#[derive(Serialize)]
pub struct SettlementPayout {
    pub user_id: i32,
    pub username: String,
    pub option_id: i32,
    pub option_text: String,
    pub shares_held: i32,
    pub payout_per_share: Decimal,
    pub total_payout: Decimal,
    pub profit_loss: Decimal,
}

#[derive(Serialize)]
pub struct SettlementResponse {
    pub event_id: i32,
    pub event_title: String,
    pub winning_option_id: i32,
    pub winning_option_text: String,
    pub total_payouts: Decimal,
    pub total_positions_settled: usize,
    pub payouts: Vec<SettlementPayout>,
    pub settlement_timestamp: chrono::NaiveDateTime,
}
