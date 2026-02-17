# Testing

## Unit Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_order_matching
```

## Integration Tests

The project includes a comprehensive Postman collection for API testing:

1. Import the collection:
   - `postman/centralized-exchange-automated.postman_collection.json`
   - `postman/centralized-exchange.postman_environment.json`

2. Run automated tests:
   ```bash
   # Using Newman (Postman CLI)
   npm install -g newman
   newman run postman/centralized-exchange-automated.postman_collection.json \
     -e postman/centralized-exchange.postman_environment.json
   ```

## Load Testing

```bash
# Using Apache Bench
ab -n 1000 -c 10 http://localhost:8080/health

# Using wrk
wrk -t12 -c400 -d30s http://localhost:8080/health
```
