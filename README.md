
# Exchange API

A high-performance, real-time centralized exchange backend built with Rust, designed for prediction markets and event-based trading. This production-ready system features a complete order matching engine, automated market making, and real-time WebSocket updates.

## Features

### Core Trading
- **Order Book Engine**: In-memory order matching (Market, Limit, IOC, FOK, GTC)
- **Automated Market Making**: Configurable liquidity provisioning for new markets
- **Real-time Price Discovery**: Continuous price updates based on order flow
- **Trade History**: Complete audit trail of all executed trades
- **Position Tracking**: Real-time portfolio and position management

### Platform
- **Multi-role Authentication**: JWT-based auth with user and admin roles
- **Event Management**: Create and manage prediction markets/events
- **Wallet System**: Built-in deposit/withdrawal functionality
- **WebSocket Support**: Real-time updates for prices, trades, and order status
- **Redis Caching**: High-performance caching layer for frequently accessed data
- **Database Persistence**: PostgreSQL storage with automatic migrations
- **Automated Testing**: Comprehensive Postman collection for API testing

## Tech Stack

- **Language**: Rust
- **Web Framework**: Actix-web 4.4
- **Database**: PostgreSQL with SeaORM 1.1
- **Caching**: Redis with deadpool-redis connection pooling
- **Async Runtime**: Tokio
- **Authentication**: JWT
- **WebSockets**: actix-web-actors
- **Serialization**: Serde
- **Security**: bcrypt for password hashing

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.75 or later)
- [PostgreSQL](https://www.postgresql.org/download/) (14 or later)
- [Redis](https://redis.io/download/) (7.0 or later)

### Quick Start with Docker

```bash
git clone https://github.com/yourusername/centralized-exchange.git
cd centralized-exchange

# Start PostgreSQL and Redis using Docker
docker-compose up -d postgres redis

# Copy environment template
cp .env.example .env

# Run database migrations
cargo run --bin migration

# Start the server
cargo run
```

### Manual Setup

#### 1. Configure Environment Variables

Create a `.env` file in the project root:

```env
SERVER_ADDRESS=127.0.0.1:8080
DATABASE_URL=postgres://user:password@localhost:5432/exchange_db
REDIS_URL=redis://127.0.0.1:6379
REDIS_MAX_CONNECTIONS=10
REDIS_TIMEOUT_SECONDS=5
JWT_SECRET=your-super-secret-jwt-key-change-this-in-production
JWT_EXPIRATION=3600
CORS_ORIGIN=http://localhost:3000
PRICE_UPDATE_INTERVAL_SECONDS=300
```

#### 2. Set Up the Database

```bash
createdb exchange_db
cargo run --bin migration
```

#### 3. Start the Server

```bash
# Development mode with hot reloading
cargo watch -x run

# Production mode
cargo run --release
```

The server will start at `http://127.0.0.1:8080`

## Documentation

| Document | Description |
|----------|-------------|
| [API Reference](docs/API.md) | Complete list of all REST API endpoints |
| [Architecture](docs/ARCHITECTURE.md) | System architecture diagram and component overview |
| [Data Models](docs/DATA_MODELS.md) | User, Event, and Order model definitions |
| [WebSocket Events](docs/WEBSOCKET.md) | Real-time WebSocket message formats |
| [Testing Guide](docs/TESTING.md) | Unit tests, integration tests, and load testing |
| [Deployment Guide](docs/DEPLOYMENT.md) | Docker deployment and production considerations |
| [Docker Setup](DOCKER_SETUP.md) | Full Docker Compose setup and troubleshooting |

## Performance

- **Order Matching**: < 1ms latency
- **WebSocket Updates**: < 10ms broadcast time
- **API Response Time**: < 50ms for most endpoints
- **Throughput**: 10,000+ orders/second on standard hardware
- **Concurrent Users**: 10,000+ concurrent WebSocket connections

## Contributing

We welcome contributions! Before committing:

- Follow Rust standard naming conventions
- Run `cargo fmt` and `cargo clippy`
- Write tests for new features

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details.

---

<p align="center">Made with ❤️ in Rust</p>
