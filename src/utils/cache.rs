use deadpool_redis::{redis::AsyncCommands, Connection, Pool};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct CacheService {
    pool: Pool,
}

impl CacheService {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Get a value from cache
    pub async fn get<T>(&self, key: &str) -> Result<Option<T>, Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut conn = self.pool.get().await?;
        let value: Option<String> = conn.get(key).await?;

        match value {
            Some(json_str) => {
                let parsed: T = serde_json::from_str(&json_str)?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    /// Set a value in cache with expiration
    pub async fn set<T>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        T: Serialize,
    {
        let mut conn = self.pool.get().await?;
        let json_str = serde_json::to_string(value)?;
        conn.set_ex(key, json_str, ttl_seconds).await?;
        Ok(())
    }

    /// Set a value in cache without expiration
    pub async fn set_persistent<T>(
        &self,
        key: &str,
        value: &T,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        T: Serialize,
    {
        let mut conn = self.pool.get().await?;
        let json_str = serde_json::to_string(value)?;
        conn.set(key, json_str).await?;
        Ok(())
    }

    /// Delete a key from cache
    pub async fn delete(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = self.pool.get().await?;
        conn.del(key).await?;
        Ok(())
    }

    /// Check if a key exists in cache
    pub async fn exists(&self, key: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let mut conn = self.pool.get().await?;
        let exists: bool = conn.exists(key).await?;
        Ok(exists)
    }

    /// Set expiration for an existing key
    pub async fn expire(
        &self,
        key: &str,
        ttl_seconds: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = self.pool.get().await?;
        conn.expire(key, ttl_seconds as i64).await?;
        Ok(())
    }

    /// Get TTL for a key
    pub async fn ttl(&self, key: &str) -> Result<i64, Box<dyn std::error::Error>> {
        let mut conn = self.pool.get().await?;
        let ttl: i64 = conn.ttl(key).await?;
        Ok(ttl)
    }

    /// Increment a numeric value
    pub async fn increment(
        &self,
        key: &str,
        increment: i64,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let mut conn = self.pool.get().await?;
        let result: i64 = conn.incr(key, increment).await?;
        Ok(result)
    }

    /// Decrement a numeric value
    pub async fn decrement(
        &self,
        key: &str,
        decrement: i64,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let mut conn = self.pool.get().await?;
        let result: i64 = conn.incr(key, -decrement).await?;
        Ok(result)
    }
}

/// Helper function to create cache keys with prefixes
pub fn create_cache_key(prefix: &str, identifier: &str) -> String {
    format!("{}:{}", prefix, identifier)
}

/// Common cache key prefixes
pub mod cache_keys {
    pub const USER_PREFIX: &str = "user";
    pub const SESSION_PREFIX: &str = "session";
    pub const EVENT_PREFIX: &str = "event";
    pub const BET_PREFIX: &str = "bet";
    pub const TRANSACTION_PREFIX: &str = "transaction";
}
