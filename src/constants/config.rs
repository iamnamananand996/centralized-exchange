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