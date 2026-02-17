# API Reference

Complete list of all REST API endpoints for the Centralized Exchange.

## Authentication Endpoints

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/auth/register` | Register new user | No |
| POST | `/auth/login` | Login user | No |

## User Management

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| GET | `/users` | List all users | No |
| GET | `/users/me` | Get current user details | Yes |
| GET | `/users/{user_id}` | Get specific user details | No |

## Transaction Management

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/transactions/deposit` | Deposit funds | Yes |
| POST | `/transactions/withdraw` | Withdraw funds | Yes |
| GET | `/transactions/transactions` | Get transaction history | Yes |

## Event Management

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| GET | `/events` | List all events | No |
| POST | `/events/create` | Create new event | Yes (Admin) |
| GET | `/events/{event_id}` | Get event details | No |
| PUT | `/events/{event_id}` | Update event | Yes (Admin) |
| POST | `/events/{event_id}/settle` | Settle event | Yes (Admin) |
| GET | `/events/{event_id}/options` | List event options | Yes |

## Event Options

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/event-options` | Create event option | Yes (Admin) |
| PUT | `/event-options/{option_id}` | Update event option | Yes (Admin) |
| GET | `/event-options/{option_id}` | Get option details | No |

## Order Book

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| POST | `/order-book/{event_id}/{option_id}/order` | Place order | Yes |
| POST | `/order-book/{event_id}/{option_id}/cancel/{order_id}` | Cancel order | Yes |
| GET | `/order-book/{event_id}/{option_id}/my-orders` | Get user's orders | Yes |
| GET | `/order-book/{event_id}/{option_id}` | Get order book | No |
| GET | `/order-book/{event_id}/{option_id}/depth` | Get market depth | No |
| GET | `/order-book/{event_id}/{option_id}/trades` | Get trade history | No |

## Portfolio & Positions

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| GET | `/portfolio` | Get user portfolio | Yes |
| GET | `/portfolio/summary` | Get portfolio summary | Yes |
| GET | `/positions/my` | Get all positions | Yes |
| GET | `/positions/{event_id}/{option_id}` | Get specific position | Yes |

## WebSocket

| Endpoint | Description |
|----------|-------------|
| `/ws/connect` | WebSocket connection for real-time updates |

See [WebSocket Events](WEBSOCKET.md) for message format details.
