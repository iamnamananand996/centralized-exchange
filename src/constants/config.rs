use std::env;

pub fn get_database_url() -> Result<String, env::VarError> {
    env::var("DATABASE_URL")
}

pub fn get_server_address() -> String {
    env::var("SERVER_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
} 

pub fn get_jwt_secret() -> String {
    env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key".to_string())
}

pub fn get_cors_origin() -> String {
    env::var("CORS_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:3000".to_string())
}

pub fn get_redis_url() -> String {
    env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string())
}

pub fn get_redis_max_connections() -> u32 {
    env::var("REDIS_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap_or(10)
}

pub fn get_redis_timeout_seconds() -> u64 {
    env::var("REDIS_TIMEOUT_SECONDS")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .unwrap_or(5)
}