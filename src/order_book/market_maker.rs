use super::{
    Order, OrderSide, OrderType, TimeInForce,
    db_persistence::DbPersistence,
    redis_persistence::RedisOrderBookPersistence,
};
use sea_orm::{DatabaseConnection, prelude::Decimal, TransactionTrait};
use deadpool_redis::Pool;
use std::error::Error;
use entity::user_positions;
use sea_orm::{ActiveModelTrait, Set, ColumnTrait, EntityTrait, QueryFilter};

#[derive(Clone, Debug)]
pub struct MarketMakerConfig {
    /// The user ID that will act as the market maker
    pub market_maker_user_id: i32,
    /// Initial price for the market (typically 50.00 for a 0-100 market)
    pub initial_price: Decimal,
    /// Spread percentage between buy and sell orders (e.g., 0.02 = 2%)
    pub spread_percentage: Decimal,
    /// Number of price levels to create on each side
    pub depth_levels: usize,
    /// Quantity at each price level
    pub level_quantity: i32,
    /// Price step between levels (e.g., 1.00)
    pub price_step: Decimal,
}

impl Default for MarketMakerConfig {
    fn default() -> Self {
        Self {
            market_maker_user_id: 1, // System user
            initial_price: Decimal::new(5000, 2), // 50.00
            spread_percentage: Decimal::new(2, 2), // 0.02 = 2%
            depth_levels: 5,
            level_quantity: 100,
            price_step: Decimal::new(100, 2), // 1.00
        }
    }
}

pub struct MarketMaker {
    config: MarketMakerConfig,
    db: DatabaseConnection,
    redis_pool: Pool,
}

impl MarketMaker {
    pub fn new(config: MarketMakerConfig, db: DatabaseConnection, redis_pool: Pool) -> Self {
        Self {
            config,
            db,
            redis_pool,
        }
    }

    /// Create initial position for the market maker to enable sell orders
    async fn create_market_maker_position(
        &self,
        event_id: i32,
        option_id: i32,
        total_shares_needed: i32,
    ) -> Result<(), Box<dyn Error>> {
        // Check if position already exists
        let existing = user_positions::Entity::find()
            .filter(user_positions::Column::UserId.eq(self.config.market_maker_user_id))
            .filter(user_positions::Column::EventId.eq(event_id))
            .filter(user_positions::Column::OptionId.eq(option_id))
            .one(&self.db)
            .await?;

        match existing {
            Some(position) => {
                // Update existing position
                let new_quantity = position.quantity + total_shares_needed;
                let mut active_position: user_positions::ActiveModel = position.into();
                active_position.quantity = Set(new_quantity);
                active_position.updated_at = Set(chrono::Utc::now().into());
                active_position.update(&self.db).await?;
                
                log::info!(
                    "Updated market maker position for option {}: {} shares (total: {})",
                    option_id,
                    total_shares_needed,
                    new_quantity
                );
            }
            None => {
                // Create new position
                let new_position = user_positions::ActiveModel {
                    user_id: Set(self.config.market_maker_user_id),
                    event_id: Set(event_id),
                    option_id: Set(option_id),
                    quantity: Set(total_shares_needed),
                    average_price: Set(Decimal::new(0, 2)), // Zero cost basis for market maker
                    created_at: Set(chrono::Utc::now().into()),
                    updated_at: Set(chrono::Utc::now().into()),
                    ..Default::default()
                };
                
                new_position.insert(&self.db).await?;
                
                log::info!(
                    "Created market maker position for option {}: {} shares",
                    option_id,
                    total_shares_needed
                );
            }
        }

        Ok(())
    }

    /// Seed initial liquidity for an event option
    pub async fn seed_initial_liquidity(
        &self,
        event_id: i32,
        option_id: i32,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        // Calculate total shares needed for sell orders
        let total_shares_needed = self.config.level_quantity * self.config.depth_levels as i32;
        
        // Create market maker position first (before creating sell orders)
        self.create_market_maker_position(event_id, option_id, total_shares_needed).await?;
        
        let redis_persistence = RedisOrderBookPersistence::new(self.redis_pool.clone());
        let db_persistence = DbPersistence::new(self.db.clone());
        
        // Get or create the order book
        let mut order_book = redis_persistence
            .get_or_create_order_book(event_id, option_id)
            .await?;

        let mut order_ids = Vec::new();

        // Calculate spread
        let half_spread = self.config.spread_percentage / Decimal::from(2);
        
        // Create sell orders (asks) above the initial price
        let ask_base_price = self.config.initial_price * (Decimal::from(1) + half_spread);
        for i in 0..self.config.depth_levels {
            let price = ask_base_price + (self.config.price_step * Decimal::from(i as i64));
            
            // Don't exceed 100.00 for prediction markets
            if price > Decimal::new(10000, 2) {
                break;
            }

            let order = Order::new(
                self.config.market_maker_user_id,
                event_id,
                option_id,
                OrderSide::Sell,
                OrderType::Limit,
                TimeInForce::GTC,
                price,
                self.config.level_quantity,
            );

            // Save to database
            db_persistence.save_order(&order).await?;
            redis_persistence.save_order(&order).await?;
            
            order_ids.push(order.id.clone());
            
            // Add to order book
            order_book.add_order_directly(order);
        }

        // Create buy orders (bids) below the initial price
        let bid_base_price = self.config.initial_price * (Decimal::from(1) - half_spread);
        for i in 0..self.config.depth_levels {
            let price = bid_base_price - (self.config.price_step * Decimal::from(i as i64));
            
            // Don't go below 0.00
            if price <= Decimal::new(0, 2) {
                break;
            }

            let order = Order::new(
                self.config.market_maker_user_id,
                event_id,
                option_id,
                OrderSide::Buy,
                OrderType::Limit,
                TimeInForce::GTC,
                price,
                self.config.level_quantity,
            );

            // Save to database
            db_persistence.save_order(&order).await?;
            redis_persistence.save_order(&order).await?;
            
            order_ids.push(order.id.clone());
            
            // Add to order book
            order_book.add_order_directly(order);
        }

        // Save the updated order book
        redis_persistence
            .save_full_order_book(event_id, option_id, &order_book)
            .await?;

        log::info!(
            "Seeded {} orders for event {} option {} with initial price {} (market maker has {} shares)",
            order_ids.len(),
            event_id,
            option_id,
            self.config.initial_price,
            total_shares_needed
        );

        Ok(order_ids)
    }

} 