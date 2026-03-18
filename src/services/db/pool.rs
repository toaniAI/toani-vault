//! PostgreSQL 数据库连接池
//!
//! 提供数据库连接管理和配置。
//! 支持 RLS（行级安全）上下文设置。

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{Postgres, Transaction, pool::PoolConnection};
use std::time::Duration;
use thiserror::Error;

use crate::utils::sql::escape_sql_string;

// 导入 RLS 上下文（避免循环依赖，使用前向声明）
// 实际类型定义在 crate::api::context 中

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

    #[error("RLS 上下文设置失败: {0}")]
    RlsContextError(String),
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
            return Err(DatabaseError::ConfigError(
                "Database URL is empty".to_string(),
            ));
        }
        if self.max_connections == 0 {
            return Err(DatabaseError::ConfigError(
                "Max connections must be greater than 0".to_string(),
            ));
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

/// RLS 上下文trait，用于抽象 RLS 设置
///
/// 实现此 trait 的类型可以用于设置数据库 RLS 上下文
pub trait RlsContextData {
    /// 获取租户ID
    fn tenant_id(&self) -> &str;
    /// 获取用户ID
    fn user_id(&self) -> &str;
    /// 获取 scopes 列表
    fn scopes(&self) -> &[String];
    /// 是否为管理员
    fn is_admin(&self) -> bool;

    /// 生成 SET LOCAL SQL 语句（事务级别）
    fn to_sql_transaction_local(&self) -> String {
        format!(
            "SET LOCAL app.current_tenant_id = '{}'; \
             SET LOCAL app.current_user_id = '{}'; \
             SET LOCAL app.current_scopes = '{}'; \
             SET LOCAL app.is_admin = '{}';",
            escape_sql_string(self.tenant_id()),
            escape_sql_string(self.user_id()),
            escape_sql_string(&self.scopes().join(",")),
            self.is_admin()
        )
    }
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

    /// 获取数据库连接
    pub async fn acquire(&self) -> Result<PoolConnection<Postgres>, DatabaseError> {
        self.pool
            .acquire()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))
    }

    /// 获取带 RLS 上下文的数据库连接
    ///
    /// 在事务中设置 RLS 上下文变量，确保租户数据隔离
    /// 注意：此方法会自动开启一个事务
    ///
    /// # Example
    /// ```ignore
    /// let rls_ctx = RlsContext::new("tenant_123", "user_456", vec!["read".to_string()]);
    /// let mut conn = db.acquire_with_rls(&rls_ctx).await?;
    /// // 现在查询会自动应用 RLS 策略
    /// let results = sqlx::query("SELECT * FROM credentials")
    ///     .fetch_all(&mut *conn)
    ///     .await?;
    /// ```
    pub async fn acquire_with_rls<R: RlsContextData>(
        &self,
        rls_context: &R,
    ) -> Result<sqlx::Transaction<'_, Postgres>, DatabaseError> {
        // 开启事务
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DatabaseError::TransactionError(e.to_string()))?;

        // 在事务中设置 RLS 上下文
        let sql = rls_context.to_sql_transaction_local();
        sqlx::query(&sql)
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::RlsContextError(e.to_string()))?;

        Ok(tx)
    }

    /// 在事务中设置 RLS 上下文
    ///
    /// 用于已有事务的场景，在事务中设置 RLS 变量
    ///
    /// # Example
    /// ```ignore
    /// let mut tx = db.pool().begin().await?;
    /// db.set_rls_in_transaction(&mut tx, &rls_context).await?;
    /// // 事务内的查询会应用 RLS 策略
    /// ```
    pub async fn set_rls_in_transaction<R: RlsContextData>(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        rls_context: &R,
    ) -> Result<(), DatabaseError> {
        let sql = rls_context.to_sql_transaction_local();
        sqlx::query(&sql)
            .execute(&mut **tx)
            .await
            .map_err(|e| DatabaseError::RlsContextError(e.to_string()))?;
        Ok(())
    }

    /// 在连接中设置 RLS 上下文（不开启事务）
    ///
    /// 使用 SET 命令设置会话级别变量
    /// 注意：需要手动清理或依赖连接池的重置
    pub async fn set_rls_on_connection<R: RlsContextData>(
        &self,
        conn: &mut PoolConnection<Postgres>,
        rls_context: &R,
    ) -> Result<(), DatabaseError> {
        let sqls = [
            format!(
                "SET app.current_tenant_id = '{}'",
                escape_sql_string(rls_context.tenant_id())
            ),
            format!(
                "SET app.current_user_id = '{}'",
                escape_sql_string(rls_context.user_id())
            ),
            format!(
                "SET app.current_scopes = '{}'",
                escape_sql_string(&rls_context.scopes().join(","))
            ),
            format!("SET app.is_admin = '{}'", rls_context.is_admin()),
        ];

        for sql in sqls {
            sqlx::query(&sql)
                .execute(&mut **conn)
                .await
                .map_err(|e| DatabaseError::RlsContextError(e.to_string()))?;
        }

        Ok(())
    }

    /// 重置连接的 RLS 上下文
    ///
    /// 清除所有 RLS 相关的会话变量
    pub async fn reset_rls_context(
        &self,
        conn: &mut PoolConnection<Postgres>,
    ) -> Result<(), DatabaseError> {
        sqlx::query("RESET app.current_tenant_id; RESET app.current_user_id; RESET app.current_scopes; RESET app.is_admin")
            .execute(&mut **conn)
            .await
            .map_err(|e| DatabaseError::RlsContextError(e.to_string()))?;
        Ok(())
    }

    /// 验证 RLS 是否已启用
    ///
    /// 检查数据库中 RLS 策略的状态
    pub async fn verify_rls_enabled(&self) -> Result<RlsStatus, DatabaseError> {
        let result: Vec<(String, bool, bool)> = sqlx::query_as(
            "SELECT relname, relrowsecurity, relforcerowsecurity
             FROM pg_class
             WHERE relname IN ('credentials', 'scope_tokens', 'audit_logs', 'user_roles', 'tenant_roles')
               AND relrowsecurity = true"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(RlsStatus {
            tables_with_rls: result.iter().map(|(name, _, _)| name.clone()).collect(),
            forced_tables: result
                .iter()
                .filter(|(_, _, forced)| *forced)
                .map(|(name, _, _)| name.clone())
                .collect(),
        })
    }
}

/// RLS 状态信息
#[derive(Debug, Clone)]
pub struct RlsStatus {
    /// 启用 RLS 的表
    pub tables_with_rls: Vec<String>,
    /// 强制 RLS 的表（包括表所有者）
    pub forced_tables: Vec<String>,
}

impl RlsStatus {
    /// 检查所有关键表是否都启用了 RLS
    pub fn is_fully_protected(&self) -> bool {
        let required_tables = ["credentials", "scope_tokens", "audit_logs", "user_roles"];
        required_tables
            .iter()
            .all(|t| self.tables_with_rls.contains(&t.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRlsContext {
        tenant_id: String,
        user_id: String,
        scopes: Vec<String>,
        is_admin: bool,
    }

    impl RlsContextData for TestRlsContext {
        fn tenant_id(&self) -> &str {
            &self.tenant_id
        }
        fn user_id(&self) -> &str {
            &self.user_id
        }
        fn scopes(&self) -> &[String] {
            &self.scopes
        }
        fn is_admin(&self) -> bool {
            self.is_admin
        }
    }

    #[test]
    fn test_rls_context_sql_generation() {
        let ctx = TestRlsContext {
            tenant_id: "tenant_123".to_string(),
            user_id: "user_456".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            is_admin: false,
        };

        let sql = ctx.to_sql_transaction_local();
        assert!(sql.contains("SET LOCAL app.current_tenant_id = 'tenant_123'"));
        assert!(sql.contains("SET LOCAL app.current_user_id = 'user_456'"));
        assert!(sql.contains("SET LOCAL app.is_admin = 'false'"));
    }

    #[test]
    fn test_sql_escape() {
        assert_eq!(
            escape_sql_string("test' OR '1'='1"),
            "test\\' OR \\'1\\'=\\'1"
        );
        assert_eq!(escape_sql_string("test\\value"), "test\\\\value");
        assert_eq!(escape_sql_string("test\nvalue"), "test\\nvalue");
    }

    #[test]
    fn test_rls_status_fully_protected() {
        let status = RlsStatus {
            tables_with_rls: vec![
                "credentials".to_string(),
                "scope_tokens".to_string(),
                "audit_logs".to_string(),
                "user_roles".to_string(),
            ],
            forced_tables: vec![],
        };
        assert!(status.is_fully_protected());

        let incomplete = RlsStatus {
            tables_with_rls: vec!["credentials".to_string()],
            forced_tables: vec![],
        };
        assert!(!incomplete.is_fully_protected());
    }
}
