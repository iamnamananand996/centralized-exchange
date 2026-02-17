use super::types::{Trade, UserPosition};
use entity::user_positions;
use sea_orm::{
    prelude::Decimal, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    Set, TransactionTrait,
};
use std::collections::HashMap;

pub struct PositionTracker {
    db: DatabaseConnection,
}

impl PositionTracker {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Get user's position for a specific event option
    pub async fn get_user_position(
        &self,
        user_id: i32,
        event_id: i32,
        option_id: i32,
    ) -> Result<UserPosition, String> {
        let position = user_positions::Entity::find()
            .filter(user_positions::Column::UserId.eq(user_id))
            .filter(user_positions::Column::EventId.eq(event_id))
            .filter(user_positions::Column::OptionId.eq(option_id))
            .one(&self.db)
            .await
            .map_err(|e| format!("Failed to get user position: {}", e))?;

        match position {
            Some(p) => Ok(UserPosition {
                user_id: p.user_id,
                event_id: p.event_id,
                option_id: p.option_id,
                quantity: p.quantity,
                average_price: p.average_price,
            }),
            None => Ok(UserPosition {
                user_id,
                event_id,
                option_id,
                quantity: 0,
                average_price: Decimal::new(0, 2),
            }),
        }
    }

    /// Get all positions for a user
    pub async fn get_user_positions(&self, user_id: i32) -> Result<Vec<UserPosition>, String> {
        let positions = user_positions::Entity::find()
            .filter(user_positions::Column::UserId.eq(user_id))
            .filter(user_positions::Column::Quantity.gt(0))
            .all(&self.db)
            .await
            .map_err(|e| format!("Failed to get user positions: {}", e))?;

        Ok(positions
            .into_iter()
            .map(|p| UserPosition {
                user_id: p.user_id,
                event_id: p.event_id,
                option_id: p.option_id,
                quantity: p.quantity,
                average_price: p.average_price,
            })
            .collect())
    }

    /// Update user positions based on a trade
    pub async fn update_positions_from_trade(&self, trade: &Trade) -> Result<(), String> {
        let txn = self
            .db
            .begin()
            .await
            .map_err(|e| format!("Failed to start transaction: {}", e))?;

        // Update buyer's position (increasing shares)
        self.update_position(
            &txn,
            trade.buyer_id,
            trade.event_id,
            trade.option_id,
            trade.quantity,
            trade.price,
        )
        .await?;

        // Update seller's position (decreasing shares)
        self.update_position(
            &txn,
            trade.seller_id,
            trade.event_id,
            trade.option_id,
            -trade.quantity,
            trade.price,
        )
        .await?;

        // Remove the balance update - this should be handled at the order/trade level
        // self.update_user_balances(&txn, trade).await?;

        txn.commit()
            .await
            .map_err(|e| format!("Failed to commit transaction: {}", e))?;

        Ok(())
    }

    /// Update a single user's position
    async fn update_position(
        &self,
        txn: &sea_orm::DatabaseTransaction,
        user_id: i32,
        event_id: i32,
        option_id: i32,
        quantity_change: i32,
        price: Decimal,
    ) -> Result<(), String> {
        let existing = user_positions::Entity::find()
            .filter(user_positions::Column::UserId.eq(user_id))
            .filter(user_positions::Column::EventId.eq(event_id))
            .filter(user_positions::Column::OptionId.eq(option_id))
            .one(txn)
            .await
            .map_err(|e| format!("Failed to find position: {}", e))?;

        match existing {
            Some(position) => {
                let old_quantity = position.quantity;
                let old_avg_price = position.average_price;
                let new_quantity = old_quantity + quantity_change;

                if new_quantity < 0 {
                    return Err("Insufficient shares to sell".to_string());
                }

                let mut active_position: user_positions::ActiveModel = position.into();

                if new_quantity == 0 {
                    // Position closed
                    active_position.quantity = Set(0);
                    active_position.average_price = Set(Decimal::new(0, 2));
                } else if quantity_change > 0 {
                    // Buying - update average price
                    let total_cost = old_avg_price * Decimal::from(old_quantity)
                        + price * Decimal::from(quantity_change);
                    let new_avg_price = total_cost / Decimal::from(new_quantity);

                    active_position.quantity = Set(new_quantity);
                    active_position.average_price = Set(new_avg_price);
                } else {
                    // Selling - quantity decreases but average price stays the same
                    active_position.quantity = Set(new_quantity);
                }

                active_position.updated_at = Set(chrono::Utc::now().into());
                active_position
                    .update(txn)
                    .await
                    .map_err(|e| format!("Failed to update position: {}", e))?;
            }
            None => {
                if quantity_change < 0 {
                    return Err("Cannot sell shares you don't own".to_string());
                }

                // Create new position
                let new_position = user_positions::ActiveModel {
                    user_id: Set(user_id),
                    event_id: Set(event_id),
                    option_id: Set(option_id),
                    quantity: Set(quantity_change),
                    average_price: Set(price),
                    created_at: Set(chrono::Utc::now().into()),
                    updated_at: Set(chrono::Utc::now().into()),
                    ..Default::default()
                };

                new_position
                    .insert(txn)
                    .await
                    .map_err(|e| format!("Failed to create position: {}", e))?;
            }
        }

        Ok(())
    }

    /// Validate if user has enough shares to sell
    pub async fn validate_sell_order(
        &self,
        user_id: i32,
        event_id: i32,
        option_id: i32,
        quantity: i32,
    ) -> Result<bool, String> {
        let position = self.get_user_position(user_id, event_id, option_id).await?;
        Ok(position.quantity >= quantity)
    }

    /// Get positions grouped by event for portfolio view
    pub async fn get_portfolio_positions(
        &self,
        user_id: i32,
    ) -> Result<HashMap<i32, Vec<UserPosition>>, String> {
        let positions = self.get_user_positions(user_id).await?;

        let mut grouped: HashMap<i32, Vec<UserPosition>> = HashMap::new();
        for position in positions {
            grouped
                .entry(position.event_id)
                .or_default()
                .push(position);
        }

        Ok(grouped)
    }
}
