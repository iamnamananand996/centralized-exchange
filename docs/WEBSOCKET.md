# WebSocket Events

Connect to the WebSocket server at `/ws/connect` for real-time updates.

## Client -> Server Messages

### Subscribe to order book updates

```json
{
  "type": "subscribe",
  "channel": "orderbook",
  "event_id": 1,
  "option_id": 1
}
```

### Unsubscribe from updates

```json
{
  "type": "unsubscribe",
  "channel": "orderbook",
  "event_id": 1,
  "option_id": 1
}
```

## Server -> Client Messages

### Order book update

```json
{
  "type": "orderbook_update",
  "event_id": 1,
  "option_id": 1,
  "bids": [...],
  "asks": [...],
  "last_price": "50.00"
}
```

### Trade notification

```json
{
  "type": "trade",
  "event_id": 1,
  "option_id": 1,
  "price": "50.00",
  "quantity": 100,
  "timestamp": "2024-01-01T12:00:00Z"
}
```

### Price update

```json
{
  "type": "price_update",
  "updates": [
    {
      "event_id": 1,
      "option_id": 1,
      "price": "50.00",
      "change_24h": "2.50"
    }
  ]
}
```
