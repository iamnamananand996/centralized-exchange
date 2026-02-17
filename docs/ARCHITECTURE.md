# Architecture

## System Overview

```mermaid
graph TD
    subgraph "Client Layer"
        A[Web App / Mobile App]
        B[Trading Bots]
        C[Admin Dashboard]
    end

    subgraph "API Gateway"
        D[Actix Web Server]
        E[WebSocket Server]
    end

    subgraph "Business Logic"
        F[Auth Middleware]
        G[Request Handlers]
        H[Order Book Engine]
        I[Market Maker]
        J[Position Tracker]
        K[Price Updater]
    end

    subgraph "Data Layer"
        L[(PostgreSQL)]
        M[(Redis Cache)]
        N[SeaORM]
        O[Redis Driver]
    end

    A --> D
    B --> D
    C --> D
    A -.-> E
    B -.-> E

    D --> F
    F --> G
    G --> H
    G --> I
    G --> J
    H --> K

    G --> N
    N --> L
    G --> O
    O --> M
    H --> O

    E -.-> A
    E -.-> B
    K -.-> E
```

## Component Description

- **Actix Web Server** — Handles all REST API requests with actor-based concurrency
- **WebSocket Server** — Manages real-time connections for live order book and price updates
- **Auth Middleware** — JWT-based authentication and role-based authorization (user/admin)
- **Order Book Engine** — In-memory order matching supporting Market, Limit, IOC, FOK, and GTC orders
- **Market Maker** — Automated liquidity provisioning for new markets
- **Position Tracker** — Real-time portfolio and position management per user
- **Price Updater** — Continuous price discovery from order flow, broadcasts via WebSocket
- **SeaORM** — Type-safe database layer for PostgreSQL persistence
- **Redis Cache** — High-performance caching for frequently accessed data (order books, prices)
