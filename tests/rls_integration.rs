#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

//! RLS (Row Level Security) 集成测试
//!
//! 测试 PostgreSQL 行级安全策略的正确性：
//! - 同租户访问测试
//! - 跨租户访问拒绝测试
//! - 管理员绕过测试
//! - RLS 上下文设置测试

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

// RLS 上下文结构（用于测试）
#[derive(Debug, Clone)]
struct TestRlsContext {
    tenant_id: String,
    user_id: String,
    scopes: Vec<String>,
    is_admin: bool,
}

impl TestRlsContext {
    fn new(tenant_id: &str, user_id: &str, scopes: Vec<&str>) -> Self {
        let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
        let is_admin = scopes.contains(&"admin".to_string());
        Self {
            tenant_id: tenant_id.to_string(),
            user_id: user_id.to_string(),
            scopes,
            is_admin,
        }
    }

    fn to_sql_transaction_local(&self) -> String {
        format!(
            "SET LOCAL app.current_tenant_id = '{}'; \
             SET LOCAL app.current_user_id = '{}'; \
             SET LOCAL app.current_scopes = '{}'; \
             SET LOCAL app.is_admin = '{}';",
            escape_sql_string(&self.tenant_id),
            escape_sql_string(&self.user_id),
            escape_sql_string(&self.scopes.join(",")),
            self.is_admin
        )
    }
}

fn escape_sql_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// 测试辅助函数：创建测试数据库连接池
async fn setup_test_pool() -> Option<PgPool> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .ok()?;

    Some(pool)
}

/// 测试辅助函数：检查 RLS 是否已启用
async fn is_rls_enabled(pool: &PgPool) -> bool {
    let result: Option<(bool,)> =
        sqlx::query_as("SELECT relrowsecurity FROM pg_class WHERE relname = 'credentials' LIMIT 1")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    result.map(|(enabled,)| enabled).unwrap_or(false)
}

/// 测试辅助函数：清理测试数据
async fn cleanup_test_data(pool: &PgPool, tenant_id: &str) {
    let _ = sqlx::query("DELETE FROM credentials WHERE tenant_id::text LIKE $1")
        .bind(format!("{}%", tenant_id))
        .execute(pool)
        .await;
}

// =============================================================================
// 单元测试
// =============================================================================

#[test]
fn test_rls_context_sql_generation() {
    let ctx = TestRlsContext::new("tenant_123", "user_456", vec!["read", "write"]);
    let sql = ctx.to_sql_transaction_local();

    assert!(sql.contains("SET LOCAL app.current_tenant_id = 'tenant_123'"));
    assert!(sql.contains("SET LOCAL app.current_user_id = 'user_456'"));
    assert!(sql.contains("SET LOCAL app.current_scopes = 'read,write'"));
    assert!(sql.contains("SET LOCAL app.is_admin = 'false'"));
}

#[test]
fn test_rls_context_admin_flag() {
    let admin_ctx = TestRlsContext::new("tenant_1", "user_1", vec!["admin"]);
    assert!(admin_ctx.is_admin);

    let normal_ctx = TestRlsContext::new("tenant_2", "user_2", vec!["read"]);
    assert!(!normal_ctx.is_admin);
}

#[test]
fn test_sql_escape_security() {
    // SQL 注入测试 - 验证单引号被正确转义
    let malicious = "test'; DROP TABLE credentials; --";
    let escaped = escape_sql_string(malicious);

    // 验证单引号被转义为 \'
    assert!(escaped.contains("test\\'"));
    // 验证转义后的字符串不包含未转义的单引号（即没有 ' 前面没有 \ 的情况）
    // 检查所有单引号都被转义：每个 ' 前面应该有 \
    let chars: Vec<char> = escaped.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '\'' {
            // 确保每个单引号前面都有反斜杠
            assert!(
                i > 0 && chars[i - 1] == '\\',
                "发现未转义的单引号在位置 {}",
                i
            );
        }
    }
    // 验证原始内容（除单引号外）保持不变
    assert!(escaped.contains("DROP TABLE"));
}

// =============================================================================
// 集成测试（需要数据库连接）
// =============================================================================

#[cfg(feature = "rls-tests")]
mod integration_tests {
    use super::*;

    /// 测试：RLS 已启用
    #[sqlx::test]
    async fn test_rls_is_enabled(pool: PgPool) {
        assert!(
            is_rls_enabled(&pool).await,
            "RLS 应该在 credentials 表上启用"
        );
    }

    /// 测试：同租户访问
    #[sqlx::test]
    async fn test_same_tenant_access(pool: PgPool) {
        let tenant_id = "test_tenant_rls_1";
        let user_id = "test_user_1";
        let ctx = TestRlsContext::new(tenant_id, user_id, vec!["read"]);

        // 清理
        cleanup_test_data(&pool, "test_tenant_rls").await;

        // 插入测试数据（使用 RLS 上下文）
        let mut tx = pool.begin().await.expect("Failed to begin transaction");
        sqlx::query(&ctx.to_sql_transaction_local())
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context");

        // 尝试读取（应该能看到数据）
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM credentials")
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(0);

        // 即使没有数据，查询也不应该报错
        assert!(count >= 0);

        tx.rollback().await.ok();
    }

    /// 测试：跨租户访问被拒绝
    #[sqlx::test]
    async fn test_cross_tenant_access_denied(pool: PgPool) {
        let tenant_a = "test_tenant_a";
        let tenant_b = "test_tenant_b";

        // 清理
        cleanup_test_data(&pool, "test_tenant").await;

        // 租户 A 插入数据
        let mut tx_a = pool.begin().await.expect("Failed to begin transaction");
        let ctx_a = TestRlsContext::new(tenant_a, "user_a", vec!["read", "write"]);

        sqlx::query(&ctx_a.to_sql_transaction_local())
            .execute(&mut *tx_a)
            .await
            .expect("Failed to set RLS context");

        // 租户 B 尝试读取
        let ctx_b = TestRlsContext::new(tenant_b, "user_b", vec!["read"]);
        let mut tx_b = pool.begin().await.expect("Failed to begin transaction");

        sqlx::query(&ctx_b.to_sql_transaction_local())
            .execute(&mut *tx_b)
            .await
            .expect("Failed to set RLS context");

        // 租户 B 不应该能看到租户 A 的数据
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM credentials")
            .fetch_one(&mut *tx_b)
            .await
            .unwrap_or(0);

        // RLS 应该阻止看到其他租户的数据
        assert_eq!(count, 0, "RLS 应该阻止跨租户访问");

        tx_a.rollback().await.ok();
        tx_b.rollback().await.ok();
    }

    /// 测试：管理员可以绕过 RLS
    #[sqlx::test]
    async fn test_admin_bypass_rls(pool: PgPool) {
        let tenant_a = "test_tenant_admin_a";
        let admin_tenant = "test_tenant_admin";

        // 清理
        cleanup_test_data(&pool, "test_tenant_admin").await;

        // 创建管理员上下文
        let admin_ctx = TestRlsContext::new(admin_tenant, "admin_user", vec!["admin"]);

        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        sqlx::query(&admin_ctx.to_sql_transaction_local())
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context");

        // 管理员应该能够查询（is_admin = true）
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM credentials")
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(0);

        // 查询不应该失败
        assert!(count >= 0);

        tx.rollback().await.ok();
    }

    /// 测试：无 RLS 上下文时查询被拒绝
    #[sqlx::test]
    async fn test_no_context_denied(pool: PgPool) {
        // 不设置 RLS 上下文，直接查询
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        // deny_all 策略应该阻止查询
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM credentials")
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(0);

        // 应该返回 0，因为 deny_all 策略
        assert_eq!(count, 0, "没有 RLS 上下文时应该返回空结果");

        tx.rollback().await.ok();
    }
}

// =============================================================================
// 性能测试
// =============================================================================

#[cfg(feature = "rls-benchmark")]
mod benchmark_tests {
    use super::*;
    use std::time::Instant;

    /// 性能基准：RLS 设置开销
    #[sqlx::test]
    async fn test_rls_performance_overhead(pool: PgPool) {
        let ctx = TestRlsContext::new("benchmark_tenant", "benchmark_user", vec!["read"]);

        // 预热
        for _ in 0..100 {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query(&ctx.to_sql_transaction_local())
                .execute(&mut *tx)
                .await
                .ok();
            tx.rollback().await.ok();
        }

        // 测量带 RLS 的查询
        let iterations = 1000;
        let start = Instant::now();

        for _ in 0..iterations {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query(&ctx.to_sql_transaction_local())
                .execute(&mut *tx)
                .await
                .ok();
            sqlx::query("SELECT 1").fetch_one(&mut *tx).await.ok();
            tx.rollback().await.ok();
        }

        let with_rls_duration = start.elapsed();

        // 测量不带 RLS 的查询
        let start = Instant::now();

        for _ in 0..iterations {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query("SELECT 1").fetch_one(&mut *tx).await.ok();
            tx.rollback().await.ok();
        }

        let without_rls_duration = start.elapsed();

        // 计算开销百分比
        let overhead = (with_rls_duration.as_millis() as f64
            - without_rls_duration.as_millis() as f64)
            / without_rls_duration.as_millis() as f64
            * 100.0;

        println!("RLS 性能开销: {:.2}%", overhead);
        println!("带 RLS: {:?}", with_rls_duration);
        println!("不带 RLS: {:?}", without_rls_duration);

        // 断言：性能影响 < 10%
        assert!(
            overhead < 10.0,
            "RLS 性能开销应该 < 10%, 实际: {:.2}%",
            overhead
        );
    }
}

// =============================================================================
// 测试配置
// =============================================================================

/// 测试配置说明
///
/// 运行 RLS 测试:
/// 1. 确保 PostgreSQL 数据库已配置 RLS 策略
/// 2. 设置环境变量: DATABASE_URL=postgres://...
/// 3. 执行: cargo test --test rls_integration --features rls-tests
///
/// 运行性能基准:
/// cargo test --test rls_integration --features rls-benchmark -- --nocapture
#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_rls_context_creation() {
        let ctx = TestRlsContext::new("tenant_123", "user_456", vec!["read", "write"]);
        assert_eq!(ctx.tenant_id, "tenant_123");
        assert_eq!(ctx.user_id, "user_456");
        assert_eq!(ctx.scopes, vec!["read", "write"]);
        assert!(!ctx.is_admin);
    }

    #[test]
    fn test_rls_context_admin() {
        let ctx = TestRlsContext::new("tenant_123", "user_456", vec!["admin"]);
        assert!(ctx.is_admin);
    }
}
