# Centralized Exchange API - Automated Testing Suite

## Files Created

### 1. **centralized-exchange-automated.postman_collection.json**
- **Purpose**: Main automated Postman collection with comprehensive test scripts
- **Features**:
  - Bulk user creation (20-50 users)
  - Automated login workflow
  - Random wallet deposits
  - Event and option creation
  - Smart betting simulation
  - Portfolio tracking
  - Live price monitoring
  - Comprehensive reporting

### 2. **postman-automation-guide.md**
- **Purpose**: Detailed documentation for using the automated collection
- **Contents**:
  - Setup instructions
  - Configuration options
  - Troubleshooting guide
  - Advanced usage scenarios
  - Best practices

### 3. **centralized-exchange.postman_environment.json**
- **Purpose**: Postman environment file with default configuration
- **Variables**:
  - `BASE_URL`: http://localhost:8080
  - `TOKEN`: Empty (populated during runtime)

### 4. **test-users-data.csv**
- **Purpose**: Sample CSV file for data-driven testing
- **Contents**: 26 pre-defined test users with:
  - Unique usernames and emails
  - Phone numbers
  - Secure passwords
  - Full names
  - Preset deposit amounts

## Quick Start Guide

### 1. Import Files into Postman

```bash
# In Postman:
1. Click "Import" button
2. Select all JSON files:
   - centralized-exchange-automated.postman_collection.json
   - centralized-exchange.postman_environment.json
3. Click "Import"
```

### 2. Run Full Automation

```bash
# In Postman:
1. Select "centralized-exchange-automated" collection
2. Click "Run" button
3. Select "Centralized Exchange - Local" environment
4. Click "Run centralized-exchange-automated"
```

### 3. Monitor Progress

- Open Postman Console (View → Show Postman Console)
- Watch real-time logs of:
  - User creation
  - Login attempts
  - Wallet deposits
  - Bet placements
  - Portfolio updates

### 4. Review Results

At the end of the run, check the console for the automation summary:
- Total users created
- Total deposits made
- Betting statistics
- P&L analysis

## Key Features of the Automation

### Dynamic User Generation
- Creates unique users with timestamps
- Prevents duplicate registration errors
- Stores user data for subsequent operations

### Intelligent Betting System
- Randomly selects events and options
- Calculates appropriate bet sizes based on wallet balance
- Simulates realistic betting patterns (70% participation rate)

### Comprehensive Testing
- Tests all major API endpoints
- Validates response structures
- Checks business logic (balances, calculations)
- Handles errors gracefully

### Workflow Automation
- Uses `postman.setNextRequest()` for flow control
- Maintains state across requests
- Implements retry logic for failures

## Customization Tips

### Change Number of Users
Edit in "Initialize Collection Variables":
```javascript
pm.collectionVariables.set('totalUsers', 50);
```

### Adjust Financial Parameters
```javascript
pm.collectionVariables.set('minDeposit', 1000);
pm.collectionVariables.set('maxDeposit', 10000);
pm.collectionVariables.set('minBetAmount', 25);
pm.collectionVariables.set('maxBetAmount', 250);
```

### Modify Betting Behavior
In "Place Bets on Popular Event":
```javascript
const usersWhoShouldBet = Math.floor(totalUsers * 0.8); // 80% participation
```

## Running with Newman (CLI)

### Basic Run
```bash
newman run centralized-exchange-automated.postman_collection.json \
  -e centralized-exchange.postman_environment.json
```

### With Data File
```bash
newman run centralized-exchange-automated.postman_collection.json \
  -e centralized-exchange.postman_environment.json \
  -d test-users-data.csv
```

### Generate Reports
```bash
newman run centralized-exchange-automated.postman_collection.json \
  -e centralized-exchange.postman_environment.json \
  --reporters cli,html,json \
  --reporter-html-export report.html \
  --reporter-json-export results.json
```

## Maintenance

### Before Each Test Run
1. Ensure API server is running
2. Check database connectivity
3. Verify Redis is operational
4. Clear test data if needed

### After Test Runs
1. Review console logs for errors
2. Check database for created records
3. Analyze performance metrics
4. Clean up test data if required

## Support

For issues:
1. Check the postman-automation-guide.md for detailed troubleshooting
2. Review API server logs
3. Verify all services are running
4. Check network connectivity

## Next Steps

1. **Load Testing**: Run multiple instances in parallel
2. **CI/CD Integration**: Add to your build pipeline
3. **Custom Scenarios**: Extend with more test cases
4. **Performance Monitoring**: Add response time tracking
5. **Error Testing**: Add negative test scenarios 