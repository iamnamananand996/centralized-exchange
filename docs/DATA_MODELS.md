# Data Models

## User

```rust
{
  id: i32,
  username: String,
  email: String,
  phone: Option<String>,
  password_hash: String,
  full_name: Option<String>,
  wallet_balance: Decimal,
  is_active: bool,
  role: String, // "user" or "admin"
  created_at: DateTime,
  updated_at: DateTime
}
```

## Event

```rust
{
  id: i32,
  title: String,
  description: String,
  category: String,
  status: String, // "draft", "active", "ended", "resolved"
  end_time: DateTime,
  min_bet_amount: Decimal,
  max_bet_amount: Decimal,
  total_volume: Decimal,
  image_url: String,
  created_by: i32,
  resolved_by: i32,
  winning_option_id: i32,
  resolution_note: String,
  resolved_at: DateTime,
  created_at: DateTime,
  updated_at: DateTime
}
```

## Order

```rust
{
  id: String,
  user_id: i32,
  event_id: i32,
  option_id: i32,
  side: String, // "Buy" or "Sell"
  order_type: String, // "Market" or "Limit"
  time_in_force: String, // "GTC", "IOC", "FOK"
  price: Decimal,
  quantity: i32,
  filled_quantity: i32,
  status: String, // "Pending", "PartiallyFilled", "Filled", "Cancelled", "Rejected"
  created_at: DateTime,
  updated_at: DateTime
}
```
