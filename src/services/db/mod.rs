//! 数据库服务模块
//!
//! 提供 PostgreSQL 数据库连接池和基础操作。
//! 支持多租户 Schema 隔离。

pub mod bootstrap;
pub mod pool;
pub mod schema;

pub use bootstrap::ensure_required_tables_on_startup;
pub use pool::{DatabaseConfig, DatabasePool};
pub use schema::SchemaManager;
