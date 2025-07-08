use actix_web::{web, Error, HttpResponse, Result};
use entity::{transaction, users};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
};
use sea_orm::prelude::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use rust_decimal::Decimal as RustDecimal;
use crate::utils::pagination::{PaginationQuery, PaginationInfo, PaginatedResponse};

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





pub async fn deposit_money(
    db: web::Data<DatabaseConnection>,
    user_id: web::ReqData<String>,
    request: web::Json<DepositRequest>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id: i32 = user_id_str.parse().map_err(|_| {
        actix_web::error::ErrorBadRequest("Invalid user ID")
    })?;

    let amount = request.amount;
    if amount <= 0.0 {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Amount must be greater than 0".to_string(),
            "status": "error"
        })));
    }

    // Start a database transaction
    let txn = db.begin().await.map_err(|e| {
        log::error!("Failed to start transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Database transaction failed")
    })?;

    // Get current user
    let user = users::Entity::find_by_id(user_id)
        .one(&txn)
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let user = match user {
        Some(u) => u,
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "User not found".to_string(),
                "status": "error"
            })))
        }
    };

    if !user.is_active {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "User account is deactivated".to_string(),
            "status": "error"
        })));
    }

    let balance_before = user.wallet_balance.to_string().parse::<f64>().unwrap_or(0.0);
    let balance_after = balance_before + amount;

    // Update user balance
    let mut user_active_model: users::ActiveModel = user.into();
    user_active_model.wallet_balance = Set(Decimal::from(RustDecimal::try_from(balance_after).unwrap()));
    user_active_model.updated_at = Set(chrono::Utc::now().naive_utc());

    let _updated_user = user_active_model
        .update(&txn)
        .await
        .map_err(|e| {
            log::error!("Failed to update user balance: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to update balance")
        })?;

    // Create transaction record
    let reference_id = Uuid::new_v4().to_string();
    let transaction = transaction::ActiveModel {
        user_id: Set(user_id),
        r#type: Set("deposit".to_string()),
        amount: Set(Decimal::from(RustDecimal::try_from(amount).unwrap())),
        balance_before: Set(Decimal::from(RustDecimal::try_from(balance_before).unwrap())),
        balance_after: Set(Decimal::from(RustDecimal::try_from(balance_after).unwrap())),
        status: Set("completed".to_string()),
        reference_id: Set(reference_id.clone()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        ..Default::default()
    };

    let _transaction = transaction
        .insert(&txn)
        .await
        .map_err(|e| {
            log::error!("Failed to create transaction record: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to create transaction")
        })?;

    // Commit transaction
    txn.commit().await.map_err(|e| {
        log::error!("Failed to commit transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to commit transaction")
    })?;

    Ok(HttpResponse::Ok().json(json!({
        "message": "Deposit successful".to_string(),
        "status": "success",
        "data": {
            "amount": amount,
            "balance_before": balance_before,
            "balance_after": balance_after,
            "reference_id": reference_id
        }
    })))
}

pub async fn withdraw_money(
    db: web::Data<DatabaseConnection>,
    user_id: web::ReqData<String>,
    request: web::Json<WithdrawRequest>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id: i32 = user_id_str.parse().map_err(|_| {
        actix_web::error::ErrorBadRequest("Invalid user ID")
    })?;

    let amount = request.amount;
    if amount <= 0.0 {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Amount must be greater than 0".to_string(),
            "status": "error"
        })));
    }

    // Start a database transaction
    let txn = db.begin().await.map_err(|e| {
        log::error!("Failed to start transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Database transaction failed")
    })?;

    // Get current user
    let user = users::Entity::find_by_id(user_id)
        .one(&txn)
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let user = match user {
        Some(u) => u,
        None => {
            return Ok(HttpResponse::NotFound().json(json!({
                "message": "User not found".to_string(),
                "status": "error"
            })))
        }
    };

    if !user.is_active {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "User account is deactivated".to_string(),
            "status": "error"
        })));
    }

    let balance_before = user.wallet_balance.to_string().parse::<f64>().unwrap_or(0.0);
    
    if balance_before < amount {
        return Ok(HttpResponse::BadRequest().json(json!({
            "message": "Insufficient balance".to_string(),
            "status": "error"
        })));
    }

    let balance_after = balance_before - amount;

    // Update user balance
    let mut user_active_model: users::ActiveModel = user.into();
    user_active_model.wallet_balance = Set(Decimal::from(RustDecimal::try_from(balance_after).unwrap()));
    user_active_model.updated_at = Set(chrono::Utc::now().naive_utc());

    let _updated_user = user_active_model
        .update(&txn)
        .await
        .map_err(|e| {
            log::error!("Failed to update user balance: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to update balance")
        })?;

    // Create transaction record
    let reference_id = Uuid::new_v4().to_string();
    let transaction = transaction::ActiveModel {
        user_id: Set(user_id),
        r#type: Set("withdraw".to_string()),
        amount: Set(Decimal::from(RustDecimal::try_from(amount).unwrap())),
        balance_before: Set(Decimal::from(RustDecimal::try_from(balance_before).unwrap())),
        balance_after: Set(Decimal::from(RustDecimal::try_from(balance_after).unwrap())),
        status: Set("completed".to_string()),
        reference_id: Set(reference_id.clone()),
        created_at: Set(chrono::Utc::now().naive_utc()),
        ..Default::default()
    };

    let _transaction = transaction
        .insert(&txn)
        .await
        .map_err(|e| {
            log::error!("Failed to create transaction record: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to create transaction")
        })?;

    // Commit transaction
    txn.commit().await.map_err(|e| {
        log::error!("Failed to commit transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to commit transaction")
    })?;

    Ok(HttpResponse::Ok().json(json!({
        "message": "Withdrawal successful".to_string(),
        "status": "success",
        "data": {
            "amount": amount,
            "balance_before": balance_before,
            "balance_after": balance_after,
            "reference_id": reference_id
        }
    })))
}

pub async fn get_transaction_history(
    db: web::Data<DatabaseConnection>,
    user_id: web::ReqData<String>,
    query: web::Query<PaginationQuery>,
) -> Result<HttpResponse, Error> {
    let user_id_str = &*user_id;
    let user_id: i32 = user_id_str.parse().map_err(|_| {
        actix_web::error::ErrorBadRequest("Invalid user ID")
    })?;

    let page = query.get_page();
    let limit = query.get_limit();
    let offset = query.get_offset();

    // Get total count
    let total_count = transaction::Entity::find()
        .filter(transaction::Column::UserId.eq(user_id))
        .count(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    // Get transactions with pagination
    let transactions = transaction::Entity::find()
        .filter(transaction::Column::UserId.eq(user_id))
        .order_by_desc(transaction::Column::CreatedAt)
        .offset(offset)
        .limit(limit)
        .all(db.get_ref())
        .await
        .map_err(|e| {
            log::error!("Database error: {}", e);
            actix_web::error::ErrorInternalServerError("Database error occurred")
        })?;

    let transaction_responses: Vec<TransactionResponse> = transactions
        .into_iter()
        .map(|t| TransactionResponse {
            id: t.id,
            user_id: t.user_id,
            r#type: t.r#type,
            amount: t.amount.to_string().parse::<f64>().unwrap_or(0.0),
            balance_before: t.balance_before.to_string().parse::<f64>().unwrap_or(0.0),
            balance_after: t.balance_after.to_string().parse::<f64>().unwrap_or(0.0),
            status: t.status,
            reference_id: t.reference_id,
            created_at: chrono::DateTime::from_naive_utc_and_offset(t.created_at, chrono::Utc),
        })
        .collect();

    let pagination_info = PaginationInfo::new(page, total_count, limit);
    let response = PaginatedResponse::new(transaction_responses, pagination_info);

    Ok(HttpResponse::Ok().json(json!({
        "message": "Transaction history retrieved successfully".to_string(),
        "status": "success",
        "data": response.data,
        "pagination": response.pagination
    })))
}
