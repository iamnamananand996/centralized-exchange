use chrono::{DateTime, Utc};
use sea_orm::prelude::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimeInForce {
    GTC, // Good Till Cancelled (default)
    IOC, // Immediate Or Cancel
    FOK, // Fill Or Kill - must fill entire order immediately or cancel
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub event_id: i32,
    pub option_id: i32,
    pub buyer_id: i32,
    pub seller_id: i32,
    pub buy_order_id: String,
    pub sell_order_id: String,
    pub price: Decimal,
    pub quantity: i32,
    pub total_amount: Decimal,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub event_id: i32,
    pub option_id: i32,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub last_trade_price: Option<Decimal>,
    pub mid_price: Option<Decimal>,
    pub spread: Option<Decimal>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: Decimal,
    pub quantity: i32,
    pub order_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDepth {
    pub price: Decimal,
    pub buy_quantity: i32,
    pub sell_quantity: i32,
    pub buy_orders: usize,
    pub sell_orders: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPosition {
    pub user_id: i32,
    pub event_id: i32,
    pub option_id: i32,
    pub quantity: i32,
    pub average_price: Decimal,
}

impl Order {
    pub fn new(
        user_id: i32,
        event_id: i32,
        option_id: i32,
        side: OrderSide,
        order_type: OrderType,
        time_in_force: TimeInForce,
        price: Decimal,
        quantity: i32,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id,
            event_id,
            option_id,
            side,
            order_type,
            time_in_force,
            price,
            quantity,
            filled_quantity: 0,
            status: OrderStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn remaining_quantity(&self) -> i32 {
        self.quantity - self.filled_quantity
    }

    pub fn is_filled(&self) -> bool {
        self.filled_quantity >= self.quantity
    }

    pub fn fill(&mut self, quantity: i32) {
        self.filled_quantity += quantity;
        if self.is_filled() {
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }
        self.updated_at = Utc::now();
    }

    pub fn cancel(&mut self) {
        self.status = OrderStatus::Cancelled;
        self.updated_at = Utc::now();
    }

    pub fn reject(&mut self) {
        self.status = OrderStatus::Rejected;
        self.updated_at = Utc::now();
    }
}
