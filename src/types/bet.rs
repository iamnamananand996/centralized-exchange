use serde::{Deserialize, Serialize};
use sea_orm::prelude::Decimal;
use chrono::{DateTime, Utc};
use entity::bets;
use crate::utils::pagination::PaginationQuery;

#[derive(Deserialize, Debug)]
pub struct PlaceBetRequest {
    pub event_id: i32,
    pub option_id: i32,
    pub quantity: i32,
    pub price_per_share: Decimal,
}

#[derive(Deserialize)]
pub struct MyBetsQuery {
    pub status: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Serialize)]
pub struct BetResponse {
    pub id: i32,
    pub event_id: i32,
    pub option_id: i32,
    pub quantity: i32,
    pub price_per_share: Decimal,
    pub total_amount: Decimal,
    pub status: String,
    pub placed_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
    pub payout_amount: Decimal,
}

impl From<bets::Model> for BetResponse {
    fn from(bet: bets::Model) -> Self {
        Self {
            id: bet.id,
            event_id: bet.event_id,
            option_id: bet.option_id,
            quantity: bet.quantity,
            price_per_share: bet.price_per_share,
            total_amount: bet.total_amount,
            status: bet.status,
            placed_at: bet.placed_at.and_utc(),
            settled_at: bet.settled_at.map(|dt| dt.and_utc()),
            payout_amount: bet.payout_amount,
        }
    }
}

#[derive(Serialize)]
pub struct EventSummary {
    pub id: i32,
    pub title: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct OptionSummary {
    pub id: i32,
    pub option_text: String,
    pub current_price: Decimal,
}

#[derive(Serialize)]
pub struct MyBetResponse {
    pub id: i32,
    pub event: EventSummary,
    pub option: OptionSummary,
    pub quantity: i32,
    pub price_per_share: Decimal,
    pub total_amount: Decimal,
    pub current_value: Decimal,
    pub pnl: Decimal,
    pub status: String,
    pub placed_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct BetsSummary {
    pub total_invested: Decimal,
    pub current_value: Decimal,
    pub total_pnl: Decimal,
}

#[derive(Serialize)]
pub struct PositionDetail {
    pub option_text: String,
    pub quantity: i32,
    pub avg_price: Decimal,
    pub current_price: Decimal,
}

#[derive(Serialize)]
pub struct ActivePosition {
    pub event_id: i32,
    pub event_title: String,
    pub invested: Decimal,
    pub current_value: Decimal,
    pub pnl: Decimal,
    pub positions: Vec<PositionDetail>,
}

#[derive(Serialize)]
pub struct PortfolioResponse {
    pub total_invested: Decimal,
    pub current_value: Decimal,
    pub total_pnl: Decimal,
    pub wallet_balance: Decimal,
    pub active_positions: Vec<ActivePosition>,
} 