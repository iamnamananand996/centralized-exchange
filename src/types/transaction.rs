use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct DepositRequest {
    pub amount: f64,
}

#[derive(Deserialize)]
pub struct WithdrawRequest {
    pub amount: f64,
}

#[derive(Serialize)]
pub struct TransactionResponse {
    pub id: i32,
    pub user_id: i32,
    pub r#type: String,
    pub amount: f64,
    pub balance_before: f64,
    pub balance_after: f64,
    pub status: String,
    pub reference_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
