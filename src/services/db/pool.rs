//! PostgreSQL 数据库连接池
//!
//! 提供数据库连接管理和配置。

use sqlx::postgres::{PgPoolOptions, PgPool};
use thiserror::Error;
use std::time::Duration;

/// 数据库错误类型
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("数据库连接失败: {0}")]
    ConnectionFailed(String),

    #[error("配置错误: {0}")]
    ConfigError(String),

    #[error("查询执行失败: {0}")]
    QueryFailed(String),

    #[error("Schema 操作失败: {0}")]
    SchemaError(String),

    #[error("事务失败: {0}")]
    TransactionError(String),
}

/// 数据库配置
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// 数据库连接 URL
    pub url: String,
    /// 最大连接数
    pub max_connections: u32,
    /// 最小连接数
    pub min_connections: u32,
    /// 连接超时（秒）
    pub connect_timeout: u64,
    /// 空闲超时（秒）
    pub idle_timeout: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 20,
            min_connections: 5,
            connect_timeout: 30,
            idle_timeout: 600,
        }
    }
}

impl DatabaseConfig {
    /// 从环境变量创建配置
    pub fn from_env() -> Result<Self, DatabaseError> {
        use std::env;

        let url = env::var("DATABASE_URL")
            .map_err(|_| DatabaseError::ConfigError("DATABASE_URL not set".to_string()))?;

        let max_connections = env::var("DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);

        let min_connections = env::var("DATABASE_MIN_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        let connect_timeout = env::var("DATABASE_CONNECT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        Ok(Self {
            url,
            max_connections,
            min_connections,
            connect_timeout,
            idle_timeout: 600,
        })
    }

    /// 验证配置
    pub fn validate(&self) -> Result<(), DatabaseError> {
        if self.url.is_empty() {
            return Err(DatabaseError::ConfigError("Database URL is empty".to_string()));
        }
        if self.max_connections == 0 {
            return Err(DatabaseError::ConfigError("Max connections must be greater than 0".to_string()));
        }
        Ok(())
    }
}

/// PostgreSQL 数据库连接池
#[derive(Clone)]
pub struct DatabasePool {
    pool: PgPool,
    config: DatabaseConfig,
}

impl DatabasePool {
    /// 创建新的数据库连接池
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        config.validate()?;

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.connect_timeout))
            .idle_timeout(Some(Duration::from_secs(config.idle_timeout)))
            .connect(&config.url)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        Ok(Self { pool, config })
    }

    /// 从环境变量创建连接池
    pub async fn from_env() -> Result<Self, DatabaseError> {
        let config = DatabaseConfig::from_env()?;
        Self::new(config).await
    }

    /// 获取连接池引用
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// 获取配置引用
    pub fn config(&self) -> &DatabaseConfig {
        &self.config
    }

    /// 执行健康检查
    pub async fn health_check(&self) -> Result<(), DatabaseError> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;
        Ok(())
    }

    /// 关闭连接池
    pub async fn close(&self) {
        self.pool.close().await;
    }
}