use super::types::{Order, OrderSide, OrderStatus, OrderType, TimeInForce, Trade};
use entity::{orders, trades};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
};

pub struct DbPersistence {
    db: DatabaseConnection,
}

impl DbPersistence {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Save an order to the database
    pub async fn save_order(&self, order: &Order) -> Result<(), String> {
        let new_order = orders::ActiveModel {
            id: Set(order.id.clone()),
            user_id: Set(order.user_id),
            event_id: Set(order.event_id),
            option_id: Set(order.option_id),
            side: Set(order.side.to_string()),
            order_type: Set(order.order_type.to_string()),
            time_in_force: Set(order.time_in_force.to_string()),
            price: Set(order.price),
            quantity: Set(order.quantity),
            filled_quantity: Set(order.filled_quantity),
            status: Set(order.status.to_string()),
            created_at: Set(order.created_at.into()),
            updated_at: Set(order.updated_at.into()),
        };

        new_order.insert(&self.db).await.map_err(|e| {
            format!("Failed to save order to database: {}", e)
        })?;

        Ok(())
    }

    /// Update order status and filled quantity
    pub async fn update_order(&self, order: &Order) -> Result<(), String> {
        let db_order = orders::Entity::find_by_id(order.id.clone())
            .one(&self.db)
            .await
            .map_err(|e| format!("Failed to find order: {}", e))?
            .ok_or("Order not found")?;

        let mut active_order: orders::ActiveModel = db_order.into();
        active_order.filled_quantity = Set(order.filled_quantity);
        active_order.status = Set(order.status.to_string());
        active_order.updated_at = Set(order.updated_at.into());

        active_order.update(&self.db).await.map_err(|e| {
            format!("Failed to update order: {}", e)
        })?;

        Ok(())
    }

    /// Save a trade to the database
    pub async fn save_trade(&self, trade: &Trade) -> Result<(), String> {
        let new_trade = trades::ActiveModel {
            id: Set(trade.id.clone()),
            event_id: Set(trade.event_id),
            option_id: Set(trade.option_id),
            buyer_id: Set(trade.buyer_id),
            seller_id: Set(trade.seller_id),
            buy_order_id: Set(trade.buy_order_id.clone()),
            sell_order_id: Set(trade.sell_order_id.clone()),
            price: Set(trade.price),
            quantity: Set(trade.quantity),
            total_amount: Set(trade.total_amount),
            timestamp: Set(trade.timestamp.into()),
        };

        new_trade.insert(&self.db).await.map_err(|e| {
            format!("Failed to save trade to database: {}", e)
        })?;

        Ok(())
    }

    /// Get user's orders
    pub async fn get_user_orders(
        &self,
        user_id: i32,
        status: Option<&str>,
        limit: u64,
    ) -> Result<Vec<Order>, String> {
        let mut query = orders::Entity::find()
            .filter(orders::Column::UserId.eq(user_id));

        if let Some(status_filter) = status {
            query = query.filter(orders::Column::Status.eq(status_filter));
        }

        let db_orders = query
            .order_by_desc(orders::Column::CreatedAt)
            .limit(limit)
            .all(&self.db)
            .await
            .map_err(|e| format!("Failed to get user orders: {}", e))?;

        Ok(db_orders
            .into_iter()
            .map(|o| Order {
                id: o.id,
                user_id: o.user_id,
                event_id: o.event_id,
                option_id: o.option_id,
                side: parse_order_side(&o.side),
                order_type: parse_order_type(&o.order_type),
                time_in_force: parse_time_in_force(&o.time_in_force),
                price: o.price,
                quantity: o.quantity,
                filled_quantity: o.filled_quantity,
                status: parse_order_status(&o.status),
                created_at: o.created_at.into(),
                updated_at: o.updated_at.into(),
            })
            .collect())
    }

    /// Get trades for an event option
    pub async fn get_event_option_trades(
        &self,
        event_id: i32,
        option_id: i32,
        limit: u64,
    ) -> Result<Vec<Trade>, String> {
        let db_trades = trades::Entity::find()
            .filter(trades::Column::EventId.eq(event_id))
            .filter(trades::Column::OptionId.eq(option_id))
            .order_by_desc(trades::Column::Timestamp)
            .limit(limit)
            .all(&self.db)
            .await
            .map_err(|e| format!("Failed to get trades: {}", e))?;

        Ok(db_trades
            .into_iter()
            .map(|t| Trade {
                id: t.id,
                event_id: t.event_id,
                option_id: t.option_id,
                buyer_id: t.buyer_id,
                seller_id: t.seller_id,
                buy_order_id: t.buy_order_id,
                sell_order_id: t.sell_order_id,
                price: t.price,
                quantity: t.quantity,
                total_amount: t.total_amount,
                timestamp: t.timestamp.into(),
            })
            .collect())
    }

    /// Get user's trades
    pub async fn get_user_trades(
        &self,
        user_id: i32,
        limit: u64,
    ) -> Result<Vec<Trade>, String> {
        let db_trades = trades::Entity::find()
            .filter(
                trades::Column::BuyerId
                    .eq(user_id)
                    .or(trades::Column::SellerId.eq(user_id)),
            )
            .order_by_desc(trades::Column::Timestamp)
            .limit(limit)
            .all(&self.db)
            .await
            .map_err(|e| format!("Failed to get user trades: {}", e))?;

        Ok(db_trades
            .into_iter()
            .map(|t| Trade {
                id: t.id,
                event_id: t.event_id,
                option_id: t.option_id,
                buyer_id: t.buyer_id,
                seller_id: t.seller_id,
                buy_order_id: t.buy_order_id,
                sell_order_id: t.sell_order_id,
                price: t.price,
                quantity: t.quantity,
                total_amount: t.total_amount,
                timestamp: t.timestamp.into(),
            })
            .collect())
    }

    /// Execute a batch of database operations in a transaction
    pub async fn execute_in_transaction<F, R>(&self, operations: F) -> Result<R, String>
    where
        F: FnOnce(&sea_orm::DatabaseTransaction) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<R, String>> + Send + '_>,
        >,
    {
        let txn = self
            .db
            .begin()
            .await
            .map_err(|e| format!("Failed to begin transaction: {}", e))?;

        match operations(&txn).await {
            Ok(result) => {
                txn.commit()
                    .await
                    .map_err(|e| format!("Failed to commit transaction: {}", e))?;
                Ok(result)
            }
            Err(e) => {
                txn.rollback()
                    .await
                    .map_err(|e| format!("Failed to rollback transaction: {}", e))?;
                Err(e)
            }
        }
    }
}

// Helper functions to parse enums from strings
fn parse_order_side(s: &str) -> OrderSide {
    match s {
        "Buy" => OrderSide::Buy,
        "Sell" => OrderSide::Sell,
        _ => OrderSide::Buy, // Default
    }
}

fn parse_order_type(s: &str) -> OrderType {
    match s {
        "Market" => OrderType::Market,
        "Limit" => OrderType::Limit,
        _ => OrderType::Limit, // Default
    }
}

fn parse_time_in_force(s: &str) -> TimeInForce {
    match s {
        "IOC" => TimeInForce::IOC,
        "FOK" => TimeInForce::FOK,
        "GTC" => TimeInForce::GTC,
        _ => TimeInForce::GTC, // Default
    }
}

fn parse_order_status(s: &str) -> OrderStatus {
    match s {
        "Pending" => OrderStatus::Pending,
        "PartiallyFilled" => OrderStatus::PartiallyFilled,
        "Filled" => OrderStatus::Filled,
        "Cancelled" => OrderStatus::Cancelled,
        "Rejected" => OrderStatus::Rejected,
        _ => OrderStatus::Pending, // Default
    }
}

// Extension methods for enum to string conversion
impl ToString for OrderSide {
    fn to_string(&self) -> String {
        match self {
            OrderSide::Buy => "Buy".to_string(),
            OrderSide::Sell => "Sell".to_string(),
        }
    }
}

impl ToString for OrderType {
    fn to_string(&self) -> String {
        match self {
            OrderType::Market => "Market".to_string(),
            OrderType::Limit => "Limit".to_string(),
        }
    }
}

impl ToString for TimeInForce {
    fn to_string(&self) -> String {
        match self {
            TimeInForce::GTC => "GTC".to_string(),
            TimeInForce::IOC => "IOC".to_string(),
            TimeInForce::FOK => "FOK".to_string(),
        }
    }
}

impl ToString for OrderStatus {
    fn to_string(&self) -> String {
        match self {
            OrderStatus::Pending => "Pending".to_string(),
            OrderStatus::PartiallyFilled => "PartiallyFilled".to_string(),
            OrderStatus::Filled => "Filled".to_string(),
            OrderStatus::Cancelled => "Cancelled".to_string(),
            OrderStatus::Rejected => "Rejected".to_string(),
        }
    }
} 