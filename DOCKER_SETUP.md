# Docker Setup Guide

## Prerequisites

- Docker installed on your system
- Docker Compose installed

## Quick Start

1. **Start all services:**
   ```bash
   docker-compose up --build
   ```

2. **Start services in background:**
   ```bash
   docker-compose up -d --build
   ```

3. **View logs:**
   ```bash
   docker-compose logs -f app
   ```

4. **Stop services:**
   ```bash
   docker-compose down
   ```

5. **Stop and remove volumes (⚠️ This will delete all data):**
   ```bash
   docker-compose down -v
   ```

## Services

### PostgreSQL Database
- **Port:** 5432
- **Database:** centralized_exchange
- **Username:** postgres
- **Password:** postgres
- **Volume:** postgres_data

### Redis Cache
- **Port:** 6379
- **Volume:** redis_data

### Application
- **Port:** 8080
- **Health endpoint:** http://localhost:8080/health (if available)
- **WebSocket:** ws://localhost:8080/ws (if available)

## Environment Variables

The following environment variables are configured in docker-compose.yml:

- `DATABASE_URL`: PostgreSQL connection string
- `REDIS_URL`: Redis connection string
- `SERVER_ADDRESS`: Server binding address (0.0.0.0:8080)
- `JWT_SECRET`: JWT signing secret (change in production!)
- `CORS_ORIGIN`: Allowed CORS origin
- `REDIS_MAX_CONNECTIONS`: Redis connection pool size
- `REDIS_TIMEOUT_SECONDS`: Redis timeout
- `PRICE_UPDATE_INTERVAL_SECONDS`: Price update interval
- `RUST_LOG`: Logging level

## Development

For development, you can override environment variables by creating a `.env` file:

```bash
# Example .env file
DATABASE_URL=postgres://postgres:postgres@localhost:5432/centralized_exchange
REDIS_URL=redis://localhost:6379
SERVER_ADDRESS=127.0.0.1:8080
JWT_SECRET=your-super-secret-jwt-key-change-this-in-production
CORS_ORIGIN=http://localhost:3000
RUST_LOG=debug
```

## Production Considerations

1. **Change default passwords** in PostgreSQL service
2. **Update JWT_SECRET** to a secure value
3. **Configure proper CORS_ORIGIN** for your frontend
4. **Set up proper logging** and monitoring
5. **Use secrets management** for sensitive data
6. **Configure backup strategies** for data volumes

## Troubleshooting

1. **Database connection issues:**
   ```bash
   docker-compose logs postgres
   ```

2. **Redis connection issues:**
   ```bash
   docker-compose logs redis
   ```

3. **Application logs:**
   ```bash
   docker-compose logs app
   ```

4. **Check service health:**
   ```bash
   docker-compose ps
   ```

5. **Rebuild application after code changes:**
   ```bash
   docker-compose build app
   docker-compose up app
   ``` 