use super::types::{
    MarketDepth, Order, OrderBookSnapshot, OrderSide, OrderStatus, OrderType, PriceLevel,
    TimeInForce, Trade,
};
use chrono::Utc;
use sea_orm::prelude::Decimal;
use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Clone)]
pub struct OrderBookEngine {
    event_id: i32,
    option_id: i32,
    // Price -> Orders at that price level (buy orders sorted desc, sell orders sorted asc)
    buy_orders: BTreeMap<Decimal, VecDeque<Order>>,
    sell_orders: BTreeMap<Decimal, VecDeque<Order>>,
    // Order ID -> Order for quick lookup
    orders_map: HashMap<String, Order>,
    // Recent trades
    trades: VecDeque<Trade>,
    last_trade_price: Option<Decimal>,
}

impl OrderBookEngine {
    pub fn new(event_id: i32, option_id: i32) -> Self {
        Self {
            event_id,
            option_id,
            buy_orders: BTreeMap::new(),
            sell_orders: BTreeMap::new(),
            orders_map: HashMap::new(),
            trades: VecDeque::with_capacity(1000),
            last_trade_price: None,
        }
    }

    /// Submit a new order to the order book
    pub fn submit_order(&mut self, mut order: Order) -> Result<Vec<Trade>, String> {
        if order.event_id != self.event_id || order.option_id != self.option_id {
            return Err("Order doesn't match this order book".to_string());
        }

        if order.quantity <= 0 {
            return Err("Order quantity must be positive".to_string());
        }

        if order.price <= Decimal::new(0, 2) {
            return Err("Order price must be positive".to_string());
        }

        // For Fill-Or-Kill orders, check if we can fill the entire order
        if order.time_in_force == TimeInForce::FOK {
            if !self.can_fill_entire_order(&order) {
                order.reject();
                return Ok(vec![]);
            }
        }

        let mut trades = Vec::new();

        // Market orders get the best available price
        if order.order_type == OrderType::Market {
            trades = self.execute_market_order(&mut order)?;
        } else {
            // Try to match limit orders immediately
            trades = self.match_order(&mut order)?;
        }

        // Handle post-match logic based on time in force
        match order.time_in_force {
            TimeInForce::FOK => {
                // For FOK orders, if not fully filled, reject and reverse trades
                if !order.is_filled() {
                    // In a real system, we would need to reverse the trades
                    // For now, this shouldn't happen as we pre-checked
                    order.reject();
                    return Err("FOK order could not be fully filled".to_string());
                }
            }
            TimeInForce::IOC => {
                // For IOC orders, cancel any unfilled portion
                if !order.is_filled() {
                    order.cancel();
                }
            }
            TimeInForce::GTC => {
                // For GTC orders, add unfilled portion to the book
                if !order.is_filled() && order.status != OrderStatus::Cancelled {
                    self.add_order_to_book(order);
                }
            }
        }

        Ok(trades)
    }

    /// Check if an order can be filled entirely
    fn can_fill_entire_order(&self, order: &Order) -> bool {
        let mut remaining = order.quantity;

        match order.side {
            OrderSide::Buy => {
                // Check sell orders (asks)
                for (price, orders) in &self.sell_orders {
                    if *price > order.price {
                        break; // Price too high
                    }
                    for sell_order in orders {
                        remaining -= sell_order.remaining_quantity().min(remaining);
                        if remaining == 0 {
                            return true;
                        }
                    }
                }
            }
            OrderSide::Sell => {
                // Check buy orders (bids)
                for (price, orders) in self.buy_orders.iter().rev() {
                    if *price < order.price {
                        break; // Price too low
                    }
                    for buy_order in orders {
                        remaining -= buy_order.remaining_quantity().min(remaining);
                        if remaining == 0 {
                            return true;
                        }
                    }
                }
            }
        }

        remaining == 0
    }

    /// Cancel an existing order
    pub fn cancel_order(&mut self, order_id: &str) -> Result<Order, String> {
        let mut order = self.orders_map.remove(order_id).ok_or("Order not found")?;

        order.cancel();

        // Remove from price level
        match order.side {
            OrderSide::Buy => {
                if let Some(orders) = self.buy_orders.get_mut(&order.price) {
                    orders.retain(|o| o.id != order.id);
                    if orders.is_empty() {
                        self.buy_orders.remove(&order.price);
                    }
                }
            }
            OrderSide::Sell => {
                if let Some(orders) = self.sell_orders.get_mut(&order.price) {
                    orders.retain(|o| o.id != order.id);
                    if orders.is_empty() {
                        self.sell_orders.remove(&order.price);
                    }
                }
            }
        }

        Ok(order)
    }

    /// Execute a market order
    fn execute_market_order(&mut self, order: &mut Order) -> Result<Vec<Trade>, String> {
        // let mut trades: Vec<Trade> = Vec::new();
        let remaining = order.remaining_quantity();

        match order.side {
            OrderSide::Buy => {
                // Buy from the lowest asks
                let mut total_cost = Decimal::new(0, 2);
                let mut total_quantity = 0;

                // Calculate average price for market order
                for (price, orders) in self.sell_orders.iter() {
                    for sell_order in orders.iter() {
                        let available = sell_order.remaining_quantity();
                        let fill_quantity = available.min(remaining - total_quantity);
                        total_cost += *price * Decimal::from(fill_quantity);
                        total_quantity += fill_quantity;

                        if total_quantity >= remaining {
                            break;
                        }
                    }
                    if total_quantity >= remaining {
                        break;
                    }
                }

                if total_quantity == 0 {
                    return Err("No liquidity available".to_string());
                }

                // Set the market order price to the average fill price
                order.price = total_cost / Decimal::from(total_quantity);
            }
            OrderSide::Sell => {
                // Sell to the highest bids
                let mut total_value = Decimal::new(0, 2);
                let mut total_quantity = 0;

                for (price, orders) in self.buy_orders.iter().rev() {
                    for buy_order in orders.iter() {
                        let available = buy_order.remaining_quantity();
                        let fill_quantity = available.min(remaining - total_quantity);
                        total_value += *price * Decimal::from(fill_quantity);
                        total_quantity += fill_quantity;

                        if total_quantity >= remaining {
                            break;
                        }
                    }
                    if total_quantity >= remaining {
                        break;
                    }
                }

                if total_quantity == 0 {
                    return Err("No liquidity available".to_string());
                }

                // Set the market order price to the average fill price
                order.price = total_value / Decimal::from(total_quantity);
            }
        }

        // Now execute with the calculated price
        self.match_order(order)
    }

    /// Match an order against the order book
    fn match_order(&mut self, order: &mut Order) -> Result<Vec<Trade>, String> {
        let mut trades = Vec::new();

        match order.side {
            OrderSide::Buy => {
                // Match against sell orders (asks)
                let prices_to_remove: Vec<Decimal> = self
                    .sell_orders
                    .iter()
                    .filter(|(price, _)| **price <= order.price)
                    .map(|(price, _)| *price)
                    .collect();

                for price in prices_to_remove {
                    if order.is_filled() {
                        break;
                    }

                    if let Some(mut orders_at_price) = self.sell_orders.remove(&price) {
                        let mut remaining_orders = VecDeque::new();

                        while let Some(mut counter_order) = orders_at_price.pop_front() {
                            if order.is_filled() {
                                remaining_orders.push_back(counter_order);
                                continue;
                            }

                            let trade = self.execute_trade(order, &mut counter_order, price)?;
                            trades.push(trade);

                            // Update or remove counter order
                            if !counter_order.is_filled() {
                                remaining_orders.push_back(counter_order.clone());
                                self.orders_map
                                    .insert(counter_order.id.clone(), counter_order);
                            } else {
                                self.orders_map.remove(&counter_order.id);
                            }
                        }

                        if !remaining_orders.is_empty() {
                            self.sell_orders.insert(price, remaining_orders);
                        }
                    }
                }
            }
            OrderSide::Sell => {
                // Match against buy orders (bids)
                let prices_to_remove: Vec<Decimal> = self
                    .buy_orders
                    .iter()
                    .rev()
                    .filter(|(price, _)| **price >= order.price)
                    .map(|(price, _)| *price)
                    .collect();

                for price in prices_to_remove {
                    if order.is_filled() {
                        break;
                    }

                    if let Some(mut orders_at_price) = self.buy_orders.remove(&price) {
                        let mut remaining_orders = VecDeque::new();

                        while let Some(mut counter_order) = orders_at_price.pop_front() {
                            if order.is_filled() {
                                remaining_orders.push_back(counter_order);
                                continue;
                            }

                            let trade = self.execute_trade(&mut counter_order, order, price)?;
                            trades.push(trade);

                            // Update or remove counter order
                            if !counter_order.is_filled() {
                                remaining_orders.push_back(counter_order.clone());
                                self.orders_map
                                    .insert(counter_order.id.clone(), counter_order);
                            } else {
                                self.orders_map.remove(&counter_order.id);
                            }
                        }

                        if !remaining_orders.is_empty() {
                            self.buy_orders.insert(price, remaining_orders);
                        }
                    }
                }
            }
        }

        Ok(trades)
    }

    /// Execute a trade between two orders
    fn execute_trade(
        &mut self,
        buy_order: &mut Order,
        sell_order: &mut Order,
        price: Decimal,
    ) -> Result<Trade, String> {
        let quantity = buy_order
            .remaining_quantity()
            .min(sell_order.remaining_quantity());

        if quantity <= 0 {
            return Err("Invalid trade quantity".to_string());
        }

        buy_order.fill(quantity);
        sell_order.fill(quantity);

        let trade = Trade {
            id: uuid::Uuid::new_v4().to_string(),
            event_id: self.event_id,
            option_id: self.option_id,
            buyer_id: buy_order.user_id,
            seller_id: sell_order.user_id,
            buy_order_id: buy_order.id.clone(),
            sell_order_id: sell_order.id.clone(),
            price,
            quantity,
            total_amount: price * Decimal::from(quantity),
            timestamp: Utc::now(),
        };

        self.last_trade_price = Some(price);
        self.trades.push_back(trade.clone());

        // Keep only last 1000 trades
        if self.trades.len() > 1000 {
            self.trades.pop_front();
        }

        Ok(trade)
    }

    /// Add an order to the order book
    fn add_order_to_book(&mut self, order: Order) {
        let price = order.price;
        let order_id = order.id.clone();

        match order.side {
            OrderSide::Buy => {
                self.buy_orders
                    .entry(price)
                    .or_insert_with(VecDeque::new)
                    .push_back(order.clone());
            }
            OrderSide::Sell => {
                self.sell_orders
                    .entry(price)
                    .or_insert_with(VecDeque::new)
                    .push_back(order.clone());
            }
        }

        self.orders_map.insert(order_id, order);
    }

    /// Get the current order book snapshot
    pub fn get_snapshot(&self) -> OrderBookSnapshot {
        let bids = self.get_bid_levels();
        let asks = self.get_ask_levels();
        let mid_price = self.calculate_mid_price();
        let spread = self.calculate_spread();

        OrderBookSnapshot {
            event_id: self.event_id,
            option_id: self.option_id,
            bids,
            asks,
            last_trade_price: self.last_trade_price,
            mid_price,
            spread,
            timestamp: Utc::now(),
        }
    }

    /// Get bid price levels
    fn get_bid_levels(&self) -> Vec<PriceLevel> {
        self.buy_orders
            .iter()
            .rev()
            .take(10) // Top 10 levels
            .map(|(price, orders)| {
                let quantity: i32 = orders.iter().map(|o| o.remaining_quantity()).sum();
                PriceLevel {
                    price: *price,
                    quantity,
                    order_count: orders.len(),
                }
            })
            .collect()
    }

    /// Get ask price levels
    fn get_ask_levels(&self) -> Vec<PriceLevel> {
        self.sell_orders
            .iter()
            .take(10) // Top 10 levels
            .map(|(price, orders)| {
                let quantity: i32 = orders.iter().map(|o| o.remaining_quantity()).sum();
                PriceLevel {
                    price: *price,
                    quantity,
                    order_count: orders.len(),
                }
            })
            .collect()
    }

    /// Calculate the mid-market price
    pub fn calculate_mid_price(&self) -> Option<Decimal> {
        let best_bid = self.get_best_bid_price()?;
        let best_ask = self.get_best_ask_price()?;
        Some((best_bid + best_ask) / Decimal::from(2))
    }

    /// Calculate the bid-ask spread
    pub fn calculate_spread(&self) -> Option<Decimal> {
        let best_bid = self.get_best_bid_price()?;
        let best_ask = self.get_best_ask_price()?;
        Some(best_ask - best_bid)
    }

    /// Get the best bid price
    pub fn get_best_bid_price(&self) -> Option<Decimal> {
        self.buy_orders.keys().next_back().copied()
    }

    /// Get the best ask price
    pub fn get_best_ask_price(&self) -> Option<Decimal> {
        self.sell_orders.keys().next().copied()
    }

    /// Calculate volume-weighted average price (VWAP) for recent trades
    pub fn calculate_vwap(&self, trade_count: usize) -> Option<Decimal> {
        if self.trades.is_empty() {
            return None;
        }

        let recent_trades: Vec<&Trade> = self
            .trades
            .iter()
            .rev()
            .take(trade_count.min(self.trades.len()))
            .collect();

        let total_value: Decimal = recent_trades
            .iter()
            .map(|t| t.price * Decimal::from(t.quantity))
            .sum();

        let total_quantity: i32 = recent_trades.iter().map(|t| t.quantity).sum();

        if total_quantity > 0 {
            Some(total_value / Decimal::from(total_quantity))
        } else {
            None
        }
    }

    /// Get market depth at different price levels
    pub fn get_market_depth(&self, levels: usize) -> Vec<MarketDepth> {
        let mut depth_map: BTreeMap<Decimal, MarketDepth> = BTreeMap::new();

        // Add buy orders
        for (price, orders) in self.buy_orders.iter().rev().take(levels) {
            let quantity: i32 = orders.iter().map(|o| o.remaining_quantity()).sum();
            depth_map.insert(
                *price,
                MarketDepth {
                    price: *price,
                    buy_quantity: quantity,
                    sell_quantity: 0,
                    buy_orders: orders.len(),
                    sell_orders: 0,
                },
            );
        }

        // Add sell orders
        for (price, orders) in self.sell_orders.iter().take(levels) {
            let quantity: i32 = orders.iter().map(|o| o.remaining_quantity()).sum();
            depth_map
                .entry(*price)
                .and_modify(|d| {
                    d.sell_quantity = quantity;
                    d.sell_orders = orders.len();
                })
                .or_insert(MarketDepth {
                    price: *price,
                    buy_quantity: 0,
                    sell_quantity: quantity,
                    buy_orders: 0,
                    sell_orders: orders.len(),
                });
        }

        depth_map.into_values().collect()
    }

    /// Get the predicted price based on order book imbalance
    pub fn get_predicted_price(&self) -> Option<Decimal> {
        // Simple prediction based on order book imbalance
        let bid_levels = self.get_bid_levels();
        let ask_levels = self.get_ask_levels();

        if bid_levels.is_empty() || ask_levels.is_empty() {
            return self.last_trade_price;
        }

        // Calculate total bid and ask volumes in top 5 levels
        let bid_volume: i32 = bid_levels.iter().take(5).map(|l| l.quantity).sum();
        let ask_volume: i32 = ask_levels.iter().take(5).map(|l| l.quantity).sum();

        let total_volume = bid_volume + ask_volume;
        if total_volume == 0 {
            return self.calculate_mid_price();
        }

        // Weight the price based on volume imbalance
        let bid_weight = Decimal::from(bid_volume) / Decimal::from(total_volume);
        let ask_weight = Decimal::from(ask_volume) / Decimal::from(total_volume);

        let best_bid = self.get_best_bid_price()?;
        let best_ask = self.get_best_ask_price()?;

        // Predicted price leans towards the side with less volume (more aggressive)
        let predicted_price = best_bid * ask_weight + best_ask * bid_weight;
        Some(predicted_price)
    }

    /// Get the internal state of the order book for persistence
    pub fn get_internal_state(
        &self,
    ) -> (
        &BTreeMap<Decimal, VecDeque<Order>>,
        &BTreeMap<Decimal, VecDeque<Order>>,
        &HashMap<String, Order>,
        Option<Decimal>,
    ) {
        (
            &self.buy_orders,
            &self.sell_orders,
            &self.orders_map,
            self.last_trade_price,
        )
    }

    /// Add an order directly to the order book (used for reconstruction from Redis)
    pub fn add_order_directly(&mut self, order: Order) {
        let order_id = order.id.clone();
        let price = order.price;

        match order.side {
            OrderSide::Buy => {
                self.buy_orders
                    .entry(price)
                    .or_insert_with(VecDeque::new)
                    .push_back(order.clone());
            }
            OrderSide::Sell => {
                self.sell_orders
                    .entry(price)
                    .or_insert_with(VecDeque::new)
                    .push_back(order.clone());
            }
        }

        self.orders_map.insert(order_id, order);
    }

    /// Set the last trade price (used for reconstruction from Redis)
    pub fn set_last_trade_price(&mut self, price: Decimal) {
        self.last_trade_price = Some(price);
    }
}
