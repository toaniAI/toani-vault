//! CredBridge MCP Server 库
//!
//! 为 AI Agent 提供安全的凭证管理接口，基于 Model Context Protocol (MCP) 标准。

pub mod handlers;
pub mod tools;
pub mod sse;
pub mod auth;
pub mod message_queue;

use anyhow::{Context, Result};

use vault_service::vault::CredentialVault;
use vault_service::audit::MemoryAuditStorage;
use vault_service::crypto::KeyHierarchy;

/// MCP Server 配置
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// 传输模式: "stdio" 或 "sse"
    pub transport: TransportMode,
    /// SSE 模式下的监听地址
    pub sse_bind_addr: String,
    /// SSE 模式下的端口
    pub sse_port: u16,
    /// 日志级别
    pub log_level: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    Stdio,
    Sse,
}

impl McpServerConfig {
    /// 从环境变量加载配置
    pub fn from_env() -> Result<Self> {
        use std::env;

        let transport = match env::var("CREDBRIDGE_MCP_TRANSPORT")
            .unwrap_or_else(|_| "stdio".to_string())
            .to_lowercase()
            .as_str()
        {
            "sse" => TransportMode::Sse,
            _ => TransportMode::Stdio,
        };

        let sse_bind_addr = env::var("CREDBRIDGE_MCP_SSE_BIND")
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        let sse_port = env::var("CREDBRIDGE_MCP_SSE_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3721);

        let log_level = env::var("CREDBRIDGE_LOG_LEVEL")
            .unwrap_or_else(|_| "info".to_string());

        Ok(Self {
            transport,
            sse_bind_addr,
            sse_port,
            log_level,
        })
    }
}

/// MCP Server 状态
pub struct McpServerState {
    /// 凭证保险库
    pub vault: CredentialVault,
    /// 审计记录器
    pub audit: MemoryAuditStorage,
    /// Token 验证器（用于测试/开发）
    pub token_key: Option<vault_service::token::PasetoKey>,
    /// 密钥层次管理器（用于 TEE 解密）
    pub key_hierarchy: tokio::sync::RwLock<KeyHierarchy>,
}

impl McpServerState {
    /// 创建新的 Server 状态（内存存储，用于开发/测试）
    pub fn new_in_memory() -> Result<Self> {
        let vault = CredentialVault::new_in_memory();
        let audit = MemoryAuditStorage::new(10000)
            .context("Failed to create audit storage")?;

        let mut key_hierarchy = KeyHierarchy::new();
        let l0_key = vault_service::crypto::HardwareRootKey::for_simulation()
            .context("Failed to generate simulation root key")?;
        key_hierarchy.initialize_master_key(&l0_key)
            .context("Failed to initialize key hierarchy")?;

        Ok(Self {
            vault,
            audit,
            token_key: None,
            key_hierarchy: tokio::sync::RwLock::new(key_hierarchy),
        })
    }

    /// 设置 Token 密钥
    pub fn with_token_key(mut self, key: vault_service::token::PasetoKey) -> Self {
        self.token_key = Some(key);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_mode_parsing() {
        let config = McpServerConfig {
            transport: TransportMode::Stdio,
            sse_bind_addr: "127.0.0.1".to_string(),
            sse_port: 3721,
            log_level: "info".to_string(),
        };
        assert_eq!(config.transport, TransportMode::Stdio);

        let config_sse = McpServerConfig {
            transport: TransportMode::Sse,
            sse_bind_addr: "0.0.0.0".to_string(),
            sse_port: 8080,
            log_level: "debug".to_string(),
        };
        assert_eq!(config_sse.transport, TransportMode::Sse);
    }

    #[tokio::test]
    async fn test_server_state_creation() {
        let state = McpServerState::new_in_memory();
        assert!(state.is_ok());

        let state = state.unwrap();
        assert!(state.token_key.is_none());
    }
}
