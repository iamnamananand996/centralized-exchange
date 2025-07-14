use super::engine::OrderBookEngine;
use super::types::{Order, OrderStatus, Trade};
use deadpool_redis::{Pool, redis::AsyncCommands};
use sea_orm::prelude::Decimal;
use serde_json;
use std::collections::VecDeque;

pub struct RedisOrderBookPersistence {
    pool: Pool,
}

impl RedisOrderBookPersistence {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Save the complete order book state to Redis
    pub async fn save_full_order_book(
        &self,
        event_id: i32,
        option_id: i32,
        order_book: &OrderBookEngine,
    ) -> Result<(), String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        let base_key = format!("orderbook:{}:{}", event_id, option_id);
        
        // Get the internal state of the order book
        let (buy_orders, sell_orders, orders_map, last_trade_price) = order_book.get_internal_state();
        
        // Start a Redis transaction
        let _: () = deadpool_redis::redis::cmd("MULTI")
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to start Redis transaction: {}", e))?;

        // Clear existing data
        let buy_orders_key = format!("{}:buys", base_key);
        let sell_orders_key = format!("{}:sells", base_key);
        let orders_map_key = format!("{}:orders", base_key);
        let metadata_key = format!("{}:metadata", base_key);
        
        let _: () = conn.del(&buy_orders_key).await.map_err(|e| format!("Failed to delete buy orders: {}", e))?;
        let _: () = conn.del(&sell_orders_key).await.map_err(|e| format!("Failed to delete sell orders: {}", e))?;
        let _: () = conn.del(&orders_map_key).await.map_err(|e| format!("Failed to delete orders map: {}", e))?;
        
        // Save buy orders (price -> list of orders)
        for (price, orders) in buy_orders {
            let price_key = format!("{}:{}", buy_orders_key, price.to_string());
            let serialized_orders = serde_json::to_string(&orders)
                .map_err(|e| format!("Failed to serialize buy orders: {}", e))?;
            let _: () = conn.set_ex(&price_key, serialized_orders, 86400).await
                .map_err(|e| format!("Failed to save buy orders at price {}: {}", price, e))?;
        }
        
        // Save sell orders (price -> list of orders)
        for (price, orders) in sell_orders {
            let price_key = format!("{}:{}", sell_orders_key, price.to_string());
            let serialized_orders = serde_json::to_string(&orders)
                .map_err(|e| format!("Failed to serialize sell orders: {}", e))?;
            let _: () = conn.set_ex(&price_key, serialized_orders, 86400).await
                .map_err(|e| format!("Failed to save sell orders at price {}: {}", price, e))?;
        }
        
        // Save orders map
        for (order_id, order) in orders_map {
            let serialized_order = serde_json::to_string(&order)
                .map_err(|e| format!("Failed to serialize order: {}", e))?;
            let _: () = conn.hset(&orders_map_key, order_id, serialized_order).await
                .map_err(|e| format!("Failed to save order to map: {}", e))?;
        }
        
        // Save metadata
        let metadata = serde_json::json!({
            "event_id": event_id,
            "option_id": option_id,
            "last_trade_price": last_trade_price,
            "last_updated": chrono::Utc::now().to_rfc3339()
        });
        let _: () = conn.set_ex(&metadata_key, metadata.to_string(), 86400).await
            .map_err(|e| format!("Failed to save metadata: {}", e))?;
        
        // Execute transaction
        let _: () = deadpool_redis::redis::cmd("EXEC")
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to execute Redis transaction: {}", e))?;

        Ok(())
    }

    /// Load the complete order book state from Redis
    pub async fn load_full_order_book(
        &self,
        event_id: i32,
        option_id: i32,
    ) -> Result<Option<OrderBookEngine>, String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        let base_key = format!("orderbook:{}:{}", event_id, option_id);
        let metadata_key = format!("{}:metadata", base_key);
        
        // Check if order book exists
        let exists: bool = conn.exists(&metadata_key).await
            .map_err(|e| format!("Failed to check order book existence: {}", e))?;
        
        if !exists {
            return Ok(None);
        }
        
        // Load metadata
        let metadata_str: String = conn.get(&metadata_key).await
            .map_err(|e| format!("Failed to load metadata: {}", e))?;
        let metadata: serde_json::Value = serde_json::from_str(&metadata_str)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?;
        
        let last_trade_price = metadata.get("last_trade_price")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<Decimal>().ok());
        
        // Create new order book
        let mut order_book = OrderBookEngine::new(event_id, option_id);
        
        // Load buy orders
        let buy_orders_key = format!("{}:buys", base_key);
        let buy_price_keys: Vec<String> = conn.keys(format!("{}:*", buy_orders_key)).await
            .map_err(|e| format!("Failed to get buy order keys: {}", e))?;
        
        for price_key in buy_price_keys {
            let price_str = price_key.split(':').last().ok_or("Invalid price key format")?;
            let price = price_str.parse::<Decimal>()
                .map_err(|e| format!("Failed to parse price: {}", e))?;
            
            let orders_str: String = conn.get(&price_key).await
                .map_err(|e| format!("Failed to load buy orders at price {}: {}", price, e))?;
            let orders: VecDeque<Order> = serde_json::from_str(&orders_str)
                .map_err(|e| format!("Failed to deserialize buy orders: {}", e))?;
            
            // Reconstruct buy orders in the order book
            for order in orders {
                order_book.add_order_directly(order);
            }
        }
        
        // Load sell orders
        let sell_orders_key = format!("{}:sells", base_key);
        let sell_price_keys: Vec<String> = conn.keys(format!("{}:*", sell_orders_key)).await
            .map_err(|e| format!("Failed to get sell order keys: {}", e))?;
        
        for price_key in sell_price_keys {
            let price_str = price_key.split(':').last().ok_or("Invalid price key format")?;
            let price = price_str.parse::<Decimal>()
                .map_err(|e| format!("Failed to parse price: {}", e))?;
            
            let orders_str: String = conn.get(&price_key).await
                .map_err(|e| format!("Failed to load sell orders at price {}: {}", price, e))?;
            let orders: VecDeque<Order> = serde_json::from_str(&orders_str)
                .map_err(|e| format!("Failed to deserialize sell orders: {}", e))?;
            
            // Reconstruct sell orders in the order book
            for order in orders {
                order_book.add_order_directly(order);
            }
        }
        
        // Set last trade price if available
        if let Some(price) = last_trade_price {
            order_book.set_last_trade_price(price);
        }
        
        Ok(Some(order_book))
    }

    /// Get or create an order book from Redis
    pub async fn get_or_create_order_book(
        &self,
        event_id: i32,
        option_id: i32,
    ) -> Result<OrderBookEngine, String> {
        // Try to load existing order book
        if let Some(order_book) = self.load_full_order_book(event_id, option_id).await? {
            return Ok(order_book);
        }
        
        // Create new order book if doesn't exist
        let order_book = OrderBookEngine::new(event_id, option_id);
        self.save_full_order_book(event_id, option_id, &order_book).await?;
        Ok(order_book)
    }

    /// Save an order book to Redis
    pub async fn save_order_book(
        &self,
        event_id: i32,
        option_id: i32,
        order_book: &OrderBookEngine,
    ) -> Result<(), String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        let key = format!("order_book:{}:{}", event_id, option_id);
        
        // Serialize the order book snapshot
        let snapshot = order_book.get_snapshot();
        let serialized = serde_json::to_string(&snapshot)
            .map_err(|e| format!("Failed to serialize order book: {}", e))?;

        // Save with expiration (1 hour)
        conn.set_ex(&key, serialized, 3600)
            .await
            .map_err(|e| format!("Failed to save order book to Redis: {}", e))?;

        Ok(())
    }

    /// Load an order book from Redis
    pub async fn load_order_book(
        &self,
        event_id: i32,
        option_id: i32,
    ) -> Result<Option<OrderBookEngine>, String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        let key = format!("order_book:{}:{}", event_id, option_id);
        
        let data: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| format!("Failed to load order book from Redis: {}", e))?;

        match data {
            Some(serialized) => {
                // For now, we can't fully reconstruct the order book from a snapshot
                // This would require storing individual orders
                // Return None to indicate we should create a new order book
                Ok(None)
            }
            None => Ok(None),
        }
    }

    /// Save an order to Redis
    pub async fn save_order(&self, order: &Order) -> Result<(), String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        // Save order details
        let order_key = format!("order:{}", order.id);
        let serialized = serde_json::to_string(order)
            .map_err(|e| format!("Failed to serialize order: {}", e))?;

        conn.set_ex(&order_key, serialized, 86400) // 24 hours
            .await
            .map_err(|e| format!("Failed to save order to Redis: {}", e))?;

        // Add to user's order list
        let user_orders_key = format!("user:{}:orders", order.user_id);
        conn.sadd(&user_orders_key, &order.id)
            .await
            .map_err(|e| format!("Failed to add order to user list: {}", e))?;

        // Add to event-option order list
        let event_orders_key = format!("event:{}:option:{}:orders", order.event_id, order.option_id);
        conn.sadd(&event_orders_key, &order.id)
            .await
            .map_err(|e| format!("Failed to add order to event list: {}", e))?;

        Ok(())
    }

    /// Load an order from Redis
    pub async fn load_order(&self, order_id: &str) -> Result<Option<Order>, String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        let order_key = format!("order:{}", order_id);
        let data: Option<String> = conn
            .get(&order_key)
            .await
            .map_err(|e| format!("Failed to load order from Redis: {}", e))?;

        match data {
            Some(serialized) => {
                let order: Order = serde_json::from_str(&serialized)
                    .map_err(|e| format!("Failed to deserialize order: {}", e))?;
                Ok(Some(order))
            }
            None => Ok(None),
        }
    }

    /// Update order status in Redis
    pub async fn update_order_status(
        &self,
        order_id: &str,
        status: OrderStatus,
        filled_quantity: i32,
    ) -> Result<(), String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        // Load existing order
        if let Some(mut order) = self.load_order(order_id).await? {
            order.status = status;
            order.filled_quantity = filled_quantity;
            order.updated_at = chrono::Utc::now();

            // Save updated order
            self.save_order(&order).await?;
        }

        Ok(())
    }

    /// Save a trade to Redis
    pub async fn save_trade(&self, trade: &Trade) -> Result<(), String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        // Save trade details
        let trade_key = format!("trade:{}", trade.id);
        let serialized = serde_json::to_string(trade)
            .map_err(|e| format!("Failed to serialize trade: {}", e))?;

        conn.set_ex(&trade_key, serialized, 2592000) // 30 days
            .await
            .map_err(|e| format!("Failed to save trade to Redis: {}", e))?;

        // Add to event-option trade list
        let event_trades_key = format!("event:{}:option:{}:trades", trade.event_id, trade.option_id);
        let score = trade.timestamp.timestamp_millis() as f64;
        
        conn.zadd(&event_trades_key, &trade.id, score)
            .await
            .map_err(|e| format!("Failed to add trade to sorted set: {}", e))?;

        // Add to user trade lists
        let buyer_trades_key = format!("user:{}:trades", trade.buyer_id);
        let seller_trades_key = format!("user:{}:trades", trade.seller_id);
        
        conn.zadd(&buyer_trades_key, &trade.id, score)
            .await
            .map_err(|e| format!("Failed to add trade to buyer list: {}", e))?;
            
        conn.zadd(&seller_trades_key, &trade.id, score)
            .await
            .map_err(|e| format!("Failed to add trade to seller list: {}", e))?;

        Ok(())
    }

    /// Get recent trades for an event option
    pub async fn get_recent_trades(
        &self,
        event_id: i32,
        option_id: i32,
        limit: isize,
    ) -> Result<Vec<Trade>, String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        let event_trades_key = format!("event:{}:option:{}:trades", event_id, option_id);
        
        // Get most recent trade IDs
        let trade_ids: Vec<String> = conn
            .zrevrange(&event_trades_key, 0, limit - 1)
            .await
            .map_err(|e| format!("Failed to get trade IDs: {}", e))?;

        let mut trades = Vec::new();
        for trade_id in trade_ids {
            if let Some(trade) = self.load_trade(&trade_id).await? {
                trades.push(trade);
            }
        }

        Ok(trades)
    }

    /// Load a trade from Redis
    async fn load_trade(&self, trade_id: &str) -> Result<Option<Trade>, String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        let trade_key = format!("trade:{}", trade_id);
        let data: Option<String> = conn
            .get(&trade_key)
            .await
            .map_err(|e| format!("Failed to load trade from Redis: {}", e))?;

        match data {
            Some(serialized) => {
                let trade: Trade = serde_json::from_str(&serialized)
                    .map_err(|e| format!("Failed to deserialize trade: {}", e))?;
                Ok(Some(trade))
            }
            None => Ok(None),
        }
    }

    /// Get user's active orders
    pub async fn get_user_orders(
        &self,
        user_id: i32,
        status_filter: Option<OrderStatus>,
    ) -> Result<Vec<Order>, String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;

        let user_orders_key = format!("user:{}:orders", user_id);
        let order_ids: Vec<String> = conn
            .smembers(&user_orders_key)
            .await
            .map_err(|e| format!("Failed to get user order IDs: {}", e))?;

        let mut orders = Vec::new();
        for order_id in order_ids {
            if let Some(order) = self.load_order(&order_id).await? {
                if let Some(ref status) = status_filter {
                    if order.status == *status {
                        orders.push(order);
                    }
                } else {
                    orders.push(order);
                }
            }
        }

        // Sort by created_at descending
        orders.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(orders)
    }

    /// Clean up expired data
    pub async fn cleanup_expired_data(&self) -> Result<(), String> {
        // Redis handles expiration automatically for keys with TTL
        // This method could be used for additional cleanup if needed
        Ok(())
    }
} 