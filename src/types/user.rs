use serde::{Deserialize, Serialize};
use entity::users;
use crate::utils::pagination::PaginationQuery;

#[derive(Deserialize)]
pub struct ListUsersQuery {
    pub is_active: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: i32,
    pub username: String,
    pub email: String,
    pub phone: Option<String>,
    pub full_name: Option<String>,
    pub wallet_balance: sea_orm::prelude::Decimal,
    pub is_active: bool,
    pub role: String,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

impl From<users::Model> for UserResponse {
    fn from(user: users::Model) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            phone: user.phone,
            full_name: user.full_name,
            wallet_balance: user.wallet_balance,
            is_active: user.is_active,
            role: user.role,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
} 