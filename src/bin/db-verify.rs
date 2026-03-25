//! 数据库连接验证工具
//!
//! 使用方法:
//!   cargo run --bin db-verify
//!
//! 或:
//!   DATABASE_URL=postgresql://dn:dnXcdYxcv56H@10.11.25.9:15432/credbridge cargo run --bin db-verify

use sqlx::Row;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("========================================");
    println!("CredBridge 数据库连接验证");
    println!("========================================\n");

    // 从环境变量或默认配置获取数据库 URL
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://dn:dnXcdYxcv56H@10.11.25.9:15432/credbridge".to_string());

    println!("数据库 URL: {database_url}");
    println!();

    // 解析 URL 显示连接信息 (简单解析)
    let url_str = database_url.clone();
    if let Some(pos) = url_str.find("://") {
        let after_scheme = &url_str[pos + 3..];
        if let Some(at_pos) = after_scheme.find('@') {
            let _creds = &after_scheme[..at_pos];
            let after_at = &after_scheme[at_pos + 1..];
            if let Some(slash_pos) = after_at.find('/') {
                let host_port = &after_at[..slash_pos];
                let db_name = &after_at[slash_pos + 1..];

                println!("连接配置:");
                if let Some(colon_pos) = host_port.rfind(':') {
                    let host = &host_port[..colon_pos];
                    let port = &host_port[colon_pos + 1..];
                    println!("  Host: {host}");
                    println!("  Port: {port}");
                } else {
                    println!("  Host: {host_port}");
                    println!("  Port: 5432");
                }
                println!("  Database: {db_name}");
                println!();
            }
        }
    }

    // 测试连接
    println!("正在连接数据库...");
    let pool = match PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url)
        .await
    {
        Ok(pool) => {
            println!("✅ 数据库连接成功!\n");
            pool
        }
        Err(sqlx::Error::Database(db_err)) => {
            let code = db_err.code();
            if code == Some(std::borrow::Cow::Borrowed("3D000")) {
                // 数据库不存在，尝试创建
                println!(
                    "⚠️  数据库 '{}' 不存在，尝试创建...",
                    db_name_from_url(&database_url)
                );

                // 连接到 postgres 数据库来创建目标数据库
                let postgres_url = database_url
                    .rsplit_once('/')
                    .map(|(base, _)| format!("{base}/postgres"))
                    .unwrap_or_else(|| {
                        "postgresql://dn:dnXcdYxcv56H@10.11.25.9:15432/postgres".to_string()
                    });

                let temp_pool = PgPoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(std::time::Duration::from_secs(10))
                    .connect(&postgres_url)
                    .await?;

                let db_name = db_name_from_url(&database_url);
                let create_sql = format!("CREATE DATABASE \"{db_name}\"");

                match sqlx::query(&create_sql).execute(&temp_pool).await {
                    Ok(_) => {
                        println!("✅ 数据库 '{db_name}' 创建成功!\n");
                        temp_pool.close().await;

                        // 重新连接到新创建的数据库
                        println!("正在重新连接...");
                        match PgPoolOptions::new()
                            .max_connections(1)
                            .acquire_timeout(std::time::Duration::from_secs(10))
                            .connect(&database_url)
                            .await
                        {
                            Ok(pool) => {
                                println!("✅ 数据库连接成功!\n");
                                pool
                            }
                            Err(e) => {
                                println!("❌ 数据库连接失败: {e}");
                                return Err(e.into());
                            }
                        }
                    }
                    Err(e) => {
                        println!("❌ 数据库创建失败: {e}");
                        println!("请手动创建数据库:");
                        println!("  CREATE DATABASE \"{db_name}\";");
                        return Err(e.into());
                    }
                }
            } else {
                println!("❌ 数据库连接失败!");
                println!("错误: {db_err}\n");
                println!("可能的原因:");
                println!("  1. PostgreSQL 服务未运行");
                println!("  2. 连接地址或端口不正确");
                println!("  3. 用户名或密码不正确");
                println!();
                return Err(sqlx::Error::Database(db_err).into());
            }
        }
        Err(e) => {
            println!("❌ 数据库连接失败!");
            println!("错误: {e}\n");
            println!("可能的原因:");
            println!("  1. PostgreSQL 服务未运行");
            println!("  2. 连接地址或端口不正确");
            println!("  3. 用户名或密码不正确");
            println!("  4. 数据库不存在");
            println!();
            return Err(e.into());
        }
    };

    // 获取数据库版本
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&pool)
        .await?;
    println!(
        "PostgreSQL 版本: {}",
        version.split_whitespace().next().unwrap_or("unknown")
    );
    println!();

    // 获取当前数据库和用户
    let row = sqlx::query(
        "SELECT current_database(), current_user, inet_server_addr(), inet_server_port()",
    )
    .fetch_one(&pool)
    .await?;

    let db_name: String = row.get(0);
    let user_name: String = row.get(1);
    let server_addr: String = row.try_get(2).unwrap_or_default();
    let server_port: i32 = row.try_get(3).unwrap_or(5432);

    println!("连接信息:");
    println!("  数据库: {db_name}");
    println!("  用户: {user_name}");
    println!(
        "  服务器地址: {}",
        if server_addr.is_empty() {
            "localhost".to_string()
        } else {
            server_addr
        }
    );
    println!("  服务器端口: {server_port}");
    println!();

    // 检查扩展
    println!("检查 PostgreSQL 扩展...");
    let extensions: Vec<String> = sqlx::query_scalar(
        "SELECT extname FROM pg_extension WHERE extname IN ('pgcrypto', 'uuid-ossp', 'citext')",
    )
    .fetch_all(&pool)
    .await?;

    if extensions.is_empty() {
        println!("  ⚠️  未找到必要的扩展");
        println!("  正在创建扩展...");

        sqlx::query("CREATE EXTENSION IF NOT EXISTS pgcrypto")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE EXTENSION IF NOT EXISTS citext")
            .execute(&pool)
            .await?;

        println!("  ✅ 扩展创建完成");
    } else {
        println!("  ✅ 已安装扩展: {}", extensions.join(", "));
    }
    println!();

    // 获取表统计
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_one(&pool)
    .await?;

    println!("数据库统计:");
    println!("  表数量: {table_count}");
    println!();

    if table_count > 0 {
        println!("现有表:");
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name"
        )
        .fetch_all(&pool)
        .await?;

        for table in tables {
            println!("  - {table}");
        }
        println!();
    }

    // 检查 schemas
    let schemas: Vec<String> = sqlx::query_scalar(
        "SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT LIKE 'pg_%' AND schema_name NOT IN ('information_schema', 'public')"
    )
    .fetch_all(&pool)
    .await?;

    if !schemas.is_empty() {
        println!("自定义 Schemas:");
        for schema in schemas {
            println!("  - {schema}");
        }
        println!();
    }

    println!("========================================");
    println!("✅ 数据库验证完成!");
    println!("========================================");
    println!();
    println!("下一步:");
    println!("  1. 启动应用程序: cargo run");
    println!("  2. 应用程序会自动创建所需的表结构");
    println!();

    Ok(())
}

/// 从数据库 URL 中提取数据库名称
fn db_name_from_url(url: &str) -> String {
    url.rsplit_once('/')
        .map(|(_, db)| db.to_string())
        .unwrap_or_else(|| "credbridge".to_string())
}
