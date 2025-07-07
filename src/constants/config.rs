use std::env;

pub fn get_database_url() -> Result<String, env::VarError> {
    env::var("DATABASE_URL")
}

pub fn get_server_address() -> String {
    env::var("SERVER_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
} 