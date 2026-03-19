//! CredBridge MCP Server
//!
//! 为 AI Agent 提供安全的凭证管理接口，基于 Model Context Protocol (MCP) 标准。
//!
//! ## 功能特性
//!
//! - **list_credentials**: 列出用户已授权的凭证列表（返回脱敏元数据）
//! - **get_credential**: 获取凭证元数据（不包含敏感内容）
//! - **decrypt_credential**: 在 TEE 内解密凭证内容（需要 decrypt 权限）
//!
//! ## 安全特性
//!
//! - Token Scope 权限验证
//! - 审计日志记录
//! - TEE 内解密操作
//! - PII 数据脱敏
//!
//! ## 启动方式
//!
//! ### stdio 模式（默认，用于本地集成）
//! ```bash
//! cargo run --bin credbridge-mcp-server
//! ```
//!
//! ### SSE 模式（用于远程/网络部署）
//! ```bash
//! CREDBRIDGE_MCP_TRANSPORT=sse cargo run --bin credbridge-mcp-server
//! ```

use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;

use credbridge_mcp_server::{McpServerConfig, McpServerState, TransportMode};
use credbridge_mcp_server::tools::CredBridgeTools;
use credbridge_mcp_server::handlers::ToolHandler;
use credbridge_mcp_server::sse::{self, SessionManager, SseAppState};
use credbridge_mcp_server::auth::{TokenConfig, TokenValidator};

/// MCP Server 主入口
#[tokio::main]
async fn main() -> Result<()> {
    // 加载配置
    let config = McpServerConfig::from_env()
        .context("Failed to load MCP server configuration")?;

    // 初始化日志
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .with_writer(std::io::stderr)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set tracing subscriber")?;

    info!("CredBridge MCP Server starting...");
    info!("Transport mode: {:?}", config.transport);

    // 初始化 Server 状态
    let state = Arc::new(McpServerState::new_in_memory()
        .context("Failed to initialize server state")?);

    info!("Server state initialized successfully");

    // 根据传输模式启动 Server
    match config.transport {
        TransportMode::Stdio => {
            info!("Starting MCP Server in stdio mode");
            run_stdio_server(state).await?;
        }
        TransportMode::Sse => {
            info!("Starting MCP Server in SSE mode on {}:{}",
                  config.sse_bind_addr, config.sse_port);
            run_sse_server(state, &config).await?;
        }
    }

    Ok(())
}

/// 运行 stdio 模式 MCP Server
async fn run_stdio_server(state: Arc<McpServerState>) -> Result<()> {
    use rmcp::transport::io::stdio;

    info!("Initializing stdio transport...");

    // 创建工具处理器
    let tools = CredBridgeTools::new(state);
    let handler = ToolHandler::new(tools);

    // 创建 stdio 传输 (返回 (Stdin, Stdout) 元组)
    let transport = stdio();

    info!("MCP Server ready, waiting for client connections...");

    // 启动服务
    let _service = rmcp::service::serve_server(handler, transport).await
        .map_err(|e| anyhow::anyhow!("Failed to start MCP server: {}", e))?;

    // 等待服务结束 - 在 stdio 模式下，当输入结束时退出
    info!("MCP Server running. Press Ctrl+C to stop.");

    // 等待中断信号
    tokio::signal::ctrl_c().await
        .context("Failed to listen for ctrl+c")?;

    info!("MCP Server shutting down");
    Ok(())
}

/// 运行 SSE 模式 MCP Server
async fn run_sse_server(state: Arc<McpServerState>, config: &McpServerConfig) -> Result<()> {
    use std::net::SocketAddr;

    // 创建工具处理器
    let tools = CredBridgeTools::new(state.clone());
    let handler = ToolHandler::new(tools);

    // 创建 Session 管理器
    let sessions = Arc::new(SessionManager::new());

    // 创建 TokenValidator（使用默认开发配置）
    let token_validator = Arc::new(TokenValidator::new(TokenConfig::default()));

    // 创建 SSE 应用状态
    let sse_state = SseAppState {
        sessions,
        handler,
        token_validator,
    };

    // 创建 SSE Router
    let sse_router = sse::create_sse_router(sse_state);

    // 创建主应用
    let app = sse_router;

    let addr: SocketAddr = format!("{}:{}", config.sse_bind_addr, config.sse_port)
        .parse()
        .context("Invalid bind address")?;

    info!("MCP SSE Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await
        .context("Failed to bind TCP listener")?;

    axum::serve(listener, app).await
        .context("Server error")?;

    Ok(())
}
