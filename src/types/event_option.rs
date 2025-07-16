use entity::event_options;
use sea_orm::prelude::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct CreateEventOptionRequest {
    pub event_id: i32,
    pub option_text: String,
    pub current_price: Option<Decimal>,
    // Optional liquidity seeding parameters
    pub seed_liquidity: Option<bool>,
    pub liquidity_config: Option<LiquidityConfig>,
}

#[derive(Deserialize, Debug)]
pub struct LiquidityConfig {
    pub spread_percentage: Option<Decimal>,
    pub depth_levels: Option<usize>,
    pub level_quantity: Option<i32>,
    pub price_step: Option<Decimal>,
}

#[derive(Deserialize)]
pub struct UpdateEventOptionRequest {
    pub option_text: Option<String>,
    pub current_price: Option<Decimal>,
    pub is_winning_option: Option<bool>,
}

#[derive(Serialize)]
pub struct EventOptionResponse {
    pub id: i32,
    pub event_id: i32,
    pub option_text: String,
    pub current_price: Decimal,
    pub total_backing: Decimal,
    pub is_winning_option: Option<bool>,
}

impl From<event_options::Model> for EventOptionResponse {
    fn from(option: event_options::Model) -> Self {
        Self {
            id: option.id,
            event_id: option.event_id,
            option_text: option.option_text,
            current_price: option.current_price,
            total_backing: option.total_backing,
            is_winning_option: option.is_winning_option,
        }
    }
}
