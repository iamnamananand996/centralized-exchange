use crate::utils::jwt::verify_jwt_token;
use actix_web::HttpRequest;
use log::{error, info};

pub fn extract_user_id_from_headers(req: &HttpRequest) -> Option<i32> {
    // First try to get token from query parameter (for Postman and other tools)
    if let Some(query_string) = req.uri().query() {
        let params: Vec<(&str, &str)> = query_string
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.split('=');
                match (parts.next(), parts.next()) {
                    (Some(key), Some(value)) => Some((key, value)),
                    _ => None,
                }
            })
            .collect();

        for (key, value) in params {
            if key == "token" {
                // Found token in query params
                match verify_jwt_token(value) {
                    Ok(user_id_str) => match user_id_str.parse::<i32>() {
                        Ok(user_id) => {
                            info!("Authenticated WebSocket user {} via query param", user_id);
                            return Some(user_id);
                        }
                        Err(_) => {
                            error!("Failed to parse user ID from token: {}", user_id_str);
                            return None;
                        }
                    },
                    Err(e) => {
                        error!("Failed to verify JWT token from query param: {}", e);
                        return None;
                    }
                }
            }
        }
    }

    // Fall back to Authorization header
    let auth_header = req.headers().get("Authorization")?;
    let auth_str = auth_header.to_str().ok()?;

    if !auth_str.starts_with("Bearer ") {
        return None;
    }

    let token = &auth_str[7..];

    // Extract user ID from token (same logic as auth middleware)
    match verify_jwt_token(token) {
        Ok(user_id_str) => match user_id_str.parse::<i32>() {
            Ok(user_id) => {
                info!(
                    "Authenticated WebSocket user {} via Authorization header",
                    user_id
                );
                Some(user_id)
            }
            Err(_) => {
                error!("Failed to parse user ID from token: {}", user_id_str);
                None
            }
        },
        Err(e) => {
            error!("Failed to extract user ID from WebSocket token: {}", e);
            None
        }
    }
}