use crate::order_book::types::{
    MarketDepth, OrderSide, OrderStatus, OrderType, PriceLevel, TimeInForce,
};
use crate::utils::pagination::PaginationQuery;
use chrono::{DateTime, Utc};
use sea_orm::prelude::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaceOrderRequest {
    pub event_id: i32,
    pub option_id: i32,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub time_in_force: Option<TimeInForce>,
    pub price: Decimal,
    pub quantity: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaceOrderResponse {
    pub success: bool,
    pub order_id: String,
    pub trades: Vec<TradeResponse>,
    pub wallet_balance: Decimal,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CancelOrderRequest {
    pub order_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderResponse {
    pub id: String,
    pub user_id: i32,
    pub event_id: i32,
    pub option_id: i32,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub time_in_force: TimeInForce,
    pub price: Decimal,
    pub quantity: i32,
    pub filled_quantity: i32,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TradeResponse {
    pub id: String,
    pub event_id: i32,
    pub option_id: i32,
    pub buyer_id: i32,
    pub seller_id: i32,
    pub price: Decimal,
    pub quantity: i32,
    pub total_amount: Decimal,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderBookResponse {
    pub event_id: i32,
    pub option_id: i32,
    pub bids: Vec<PriceLevelResponse>,
    pub asks: Vec<PriceLevelResponse>,
    pub last_trade_price: Option<Decimal>,
    pub mid_price: Option<Decimal>,
    pub spread: Option<Decimal>,
    pub predicted_price: Option<Decimal>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceLevelResponse {
    pub price: Decimal,
    pub quantity: i32,
    pub order_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketDepthResponse {
    pub event_id: i32,
    pub option_id: i32,
    pub depth: Vec<MarketDepth>,
    pub total_bid_volume: i32,
    pub total_ask_volume: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MyOrdersQuery {
    pub status: Option<OrderStatus>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderHistoryQuery {
    pub event_id: Option<i32>,
    pub option_id: Option<i32>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

impl From<PriceLevel> for PriceLevelResponse {
    fn from(level: PriceLevel) -> Self {
        Self {
            price: level.price,
            quantity: level.quantity,
            order_count: level.order_count,
        }
    }
}
