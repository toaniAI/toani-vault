//! WebSocket API for TEE Sandbox
//!
//! 提供安全的WebSocket连接，用于实时控制和监控沙箱会话
//!
//! # 功能
//! - 执行操作 (execute)
//! - 心跳保活 (heartbeat)
//! - 操作进度通知
//! - 操作完成通知
//!
//! # 消息协议
//!
//! ## 客户端消息 (ClientMessage)
//! - `execute`: 执行沙箱操作
//! - `heartbeat`: 心跳保活
//!
//! ## 服务端消息 (ServerMessage)
//! - `connected`: 连接成功
//! - `operation_progress`: 操作进度更新
//! - `operation_completed`: 操作完成
//! - `heartbeat_ack`: 心跳确认
//! - `error`: 错误通知

use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::api::context::ApiContext;
use crate::api::middleware::ValidatedToken;
use crate::tee::sandbox::types::{ExecutionResult, OperationRequest, OperationType, SessionId};

/// 客户端消息类型
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// 执行操作
    Execute {
        /// 操作ID（可选，如果不提供则自动生成）
        operation_id: Option<String>,
        /// 操作类型
        operation_type: String,
        /// 操作描述
        description: String,
        /// 操作参数
        #[serde(default)]
        parameters: HashMap<String, serde_json::Value>,
    },
    /// 心跳消息
    Heartbeat {
        /// 客户端时间戳
        timestamp: i64,
    },
    /// 关闭连接
    Close {
        /// 关闭原因
        reason: Option<String>,
    },
}

/// 服务端消息类型
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// 连接成功
    Connected {
        /// 会话ID
        session_id: String,
        /// 连接时间
        connected_at: String,
        /// 心跳间隔（秒）
        heartbeat_interval: u64,
    },
    /// 操作进度更新
    OperationProgress {
        /// 操作ID
        operation_id: String,
        /// 操作类型
        operation_type: String,
        /// 当前状态
        status: String,
        /// 进度百分比 (0-100)
        progress: u8,
        /// 进度消息
        message: Option<String>,
        /// 时间戳
        timestamp: String,
    },
    /// 操作完成
    OperationCompleted {
        /// 操作ID
        operation_id: String,
        /// 操作类型
        operation_type: String,
        /// 是否成功
        success: bool,
        /// 返回数据
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
        /// 错误信息
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// 执行时间（毫秒）
        execution_time_ms: u64,
        /// 时间戳
        timestamp: String,
    },
    /// 心跳确认
    HeartbeatAck {
        /// 客户端时间戳
        client_timestamp: i64,
        /// 服务端时间戳
        server_timestamp: i64,
    },
    /// 会话状态更新
    SessionStatusUpdate {
        /// 会话ID
        session_id: String,
        /// 当前状态
        status: String,
        /// 消息
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// 时间戳
        timestamp: String,
    },
    /// 错误通知
    Error {
        /// 错误码
        code: String,
        /// 错误消息
        message: String,
        /// 相关操作ID（如果有）
        #[serde(skip_serializing_if = "Option::is_none")]
        operation_id: Option<String>,
        /// 时间戳
        timestamp: String,
    },
}

/// WebSocket连接状态
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ConnectionState {
    /// 会话ID
    session_id: SessionId,
    /// 租户ID
    tenant_id: Uuid,
    /// 用户ID
    user_id: Uuid,
    /// 凭证ID
    credential_id: Uuid,
    /// 连接时间
    connected_at: chrono::DateTime<chrono::Utc>,
    /// 最后心跳时间
    last_heartbeat: chrono::DateTime<chrono::Utc>,
    /// 当前操作ID
    current_operation: Option<String>,
}

/// WebSocket配置
pub struct WebSocketConfig {
    /// 心跳间隔（秒）
    pub heartbeat_interval: u64,
    /// 操作超时（秒）
    pub operation_timeout: u64,
    /// 连接超时（秒）
    pub connection_timeout: u64,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: 30,
            operation_timeout: 60,
            connection_timeout: 300,
        }
    }
}

/// 沙箱WebSocket处理器
pub async fn sandbox_websocket_handler(
    Path((session_id, credential_id)): Path<(String, String)>,
    State(ctx): State<ApiContext>,
    ws: WebSocketUpgrade,
    token: ValidatedToken,
) -> Response {
    info!(
        "WebSocket upgrade request for session: {}, credential: {}",
        session_id, credential_id
    );

    ws.on_upgrade(move |socket| handle_socket(socket, ctx, session_id, credential_id, token))
}

/// 处理WebSocket连接
pub async fn handle_socket(
    mut socket: WebSocket,
    ctx: ApiContext,
    session_id_str: String,
    credential_id_str: String,
    token: ValidatedToken,
) {
    // 解析会话ID和凭证ID
    let session_id = match Uuid::parse_str(&session_id_str) {
        Ok(id) => SessionId::from(id),
        Err(e) => {
            error!("Invalid session ID: {}", e);
            let error_msg = ServerMessage::Error {
                code: "invalid_session_id".to_string(),
                message: format!("Invalid session ID: {e}"),
                operation_id: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let _ = send_message(&mut socket, error_msg).await;
            return;
        }
    };

    let credential_id = match Uuid::parse_str(&credential_id_str) {
        Ok(id) => id,
        Err(e) => {
            error!("Invalid credential ID: {}", e);
            let error_msg = ServerMessage::Error {
                code: "invalid_credential_id".to_string(),
                message: format!("Invalid credential ID: {e}"),
                operation_id: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let _ = send_message(&mut socket, error_msg).await;
            return;
        }
    };

    // 从token获取租户ID和用户ID
    let (tenant_id, user_id) = match parse_token_subject(&token) {
        Some((t, u)) => (t, u),
        None => {
            error!("Failed to parse token subject");
            let error_msg = ServerMessage::Error {
                code: "invalid_token".to_string(),
                message: "Invalid token format".to_string(),
                operation_id: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let _ = send_message(&mut socket, error_msg).await;
            return;
        }
    };

    // 初始化连接状态
    let config = WebSocketConfig::default();
    let connected_at = chrono::Utc::now();
    let state = ConnectionState {
        session_id,
        tenant_id,
        user_id,
        credential_id,
        connected_at,
        last_heartbeat: connected_at,
        current_operation: None,
    };

    info!(
        "WebSocket connected - session: {}, tenant: {}, user: {}",
        session_id, tenant_id, user_id
    );

    // 发送连接成功消息
    let connected_msg = ServerMessage::Connected {
        session_id: session_id.to_string(),
        connected_at: connected_at.to_rfc3339(),
        heartbeat_interval: config.heartbeat_interval,
    };

    if let Err(e) = send_message(&mut socket, connected_msg).await {
        error!("Failed to send connected message: {}", e);
        return;
    }

    // 创建消息通道
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);

    // 启动心跳任务
    let _heartbeat_tx = tx.clone();
    let heartbeat_interval = config.heartbeat_interval;
    let heartbeat_handle = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(heartbeat_interval));
        loop {
            interval.tick().await;
            // 心跳由客户端发起，服务端只响应
            // 这里可以添加超时检查逻辑
        }
    });

    // 主消息循环
    loop {
        tokio::select! {
            // 接收WebSocket消息
            Some(msg) = socket.recv() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        debug!("Received text message: {}", text);
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                if let Err(e) = handle_message(
                                    client_msg,
                                    &mut socket,
                                    &state,
                                    &ctx,
                                    &config,
                                    tx.clone(),
                                ).await {
                                    warn!("Error handling message: {}", e);
                                }
                            }
                            Err(e) => {
                                let error_msg = ServerMessage::Error {
                                    code: "invalid_message".to_string(),
                                    message: format!("Failed to parse message: {e}"),
                                    operation_id: None,
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                };
                                let _ = send_message(&mut socket, error_msg).await;
                            }
                        }
                    }
                    Ok(Message::Binary(_)) => {
                        debug!("Received binary message (ignored)");
                    }
                    Ok(Message::Ping(data)) => {
                        debug!("Received ping");
                        if let Err(e) = socket.send(Message::Pong(data)).await {
                            error!("Failed to send pong: {}", e);
                            break;
                        }
                    }
                    Ok(Message::Pong(_)) => {
                        debug!("Received pong");
                    }
                    Ok(Message::Close(frame)) => {
                        info!("Client closed connection: {:?}", frame);
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
            // 发送服务端消息
            Some(msg) = rx.recv() => {
                if let Err(e) = send_message(&mut socket, msg).await {
                    error!("Failed to send message: {}", e);
                    break;
                }
            }
            // 超时检查
            _ = tokio::time::sleep(Duration::from_secs(config.connection_timeout)) => {
                warn!("Connection timeout");
                let timeout_msg = ServerMessage::Error {
                    code: "connection_timeout".to_string(),
                    message: "Connection timed out due to inactivity".to_string(),
                    operation_id: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                let _ = send_message(&mut socket, timeout_msg).await;
                break;
            }
        }
    }

    // 清理
    heartbeat_handle.abort();
    info!("WebSocket connection closed for session: {}", session_id);
}

/// 处理客户端消息
async fn handle_message(
    msg: ClientMessage,
    socket: &mut WebSocket,
    state: &ConnectionState,
    ctx: &ApiContext,
    config: &WebSocketConfig,
    tx: mpsc::Sender<ServerMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match msg {
        ClientMessage::Execute {
            operation_id,
            operation_type,
            description,
            parameters,
        } => {
            let op_id = operation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
            info!(
                "Executing operation {} (type: {}) in session {}",
                op_id, operation_type, state.session_id
            );

            // 发送进度更新 - 开始
            let progress_msg = ServerMessage::OperationProgress {
                operation_id: op_id.clone(),
                operation_type: operation_type.clone(),
                status: "executing".to_string(),
                progress: 0,
                message: Some("Starting operation...".to_string()),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let _ = tx.send(progress_msg).await;

            // 执行操作（带超时）
            let op_type = parse_operation_type(&operation_type);
            let operation = OperationRequest {
                operation_id: Uuid::parse_str(&op_id)?,
                operation_type: op_type,
                description: description.clone(),
                parameters,
                resolved_parameters: HashMap::new(),
                sensitive_output_values: Vec::new(),
                created_at: time::OffsetDateTime::now_utc(),
            };

            // 模拟操作执行（实际实现需要调用沙箱会话）
            let result = execute_operation_with_timeout(
                operation,
                state,
                ctx,
                config.operation_timeout,
                tx.clone(),
            )
            .await;

            // 发送完成消息
            let completed_msg = match result {
                Ok(exec_result) => ServerMessage::OperationCompleted {
                    operation_id: op_id.clone(),
                    operation_type: operation_type.clone(),
                    success: exec_result.success,
                    data: exec_result.data,
                    error: exec_result.error,
                    execution_time_ms: exec_result.execution_time_ms,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
                Err(e) => ServerMessage::OperationCompleted {
                    operation_id: op_id.clone(),
                    operation_type: operation_type.clone(),
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                    execution_time_ms: 0,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            };
            let _ = tx.send(completed_msg).await;
        }
        ClientMessage::Heartbeat { timestamp } => {
            debug!("Heartbeat received from client: {}", timestamp);

            let ack = ServerMessage::HeartbeatAck {
                client_timestamp: timestamp,
                server_timestamp: chrono::Utc::now().timestamp_millis(),
            };
            send_message(socket, ack).await?;
        }
        ClientMessage::Close { reason } => {
            info!("Client requested close: {:?}", reason);
            // 关闭连接
            return Err("Client requested close".into());
        }
    }

    Ok(())
}

/// 发送服务端消息
async fn send_message(
    socket: &mut WebSocket,
    msg: ServerMessage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let json = serde_json::to_string(&msg)?;
    socket.send(Message::Text(json)).await?;
    Ok(())
}

/// 解析token获取租户ID和用户ID
fn parse_token_subject(token: &ValidatedToken) -> Option<(Uuid, Uuid)> {
    // 从token中直接获取tenant_id和user_id
    // 格式: "tenant_id:user_id"
    if let (Ok(tenant_id), Ok(user_id)) = (
        Uuid::parse_str(&token.tenant_id),
        Uuid::parse_str(&token.user_id),
    ) {
        return Some((tenant_id, user_id));
    }
    None
}

/// 解析操作类型
fn parse_operation_type(op_type: &str) -> OperationType {
    match op_type.to_lowercase().as_str() {
        "httprequest" | "http_request" => OperationType::HttpRequest,
        _ => OperationType::Custom,
    }
}

/// 执行操作（带超时和进度更新）
async fn execute_operation_with_timeout(
    operation: OperationRequest,
    state: &ConnectionState,
    _ctx: &ApiContext,
    timeout_secs: u64,
    tx: mpsc::Sender<ServerMessage>,
) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
    let _start = std::time::Instant::now();
    let op_id = operation.operation_id.to_string();
    let op_type = operation.operation_type.to_string();

    // 模拟进度更新
    let progress_tx = tx.clone();
    let progress_handle = tokio::spawn(async move {
        for i in [25, 50, 75] {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let progress_msg = ServerMessage::OperationProgress {
                operation_id: op_id.clone(),
                operation_type: op_type.clone(),
                status: "executing".to_string(),
                progress: i,
                message: Some(format!("Progress {i}%")),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let _ = progress_tx.send(progress_msg).await;
        }
    });

    // 实际执行操作（带超时）
    let result = timeout(
        Duration::from_secs(timeout_secs),
        execute_operation(operation, state),
    )
    .await;

    progress_handle.abort();

    match result {
        Ok(Ok(exec_result)) => Ok(exec_result),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Operation timeout".into()),
    }
}

/// 执行操作
///
/// 当前实现：沙箱会话池尚未集成到 ConnectionState，
/// 记录操作请求并返回"待实现"错误，避免误导性的假成功响应。
/// 集成点：ConnectionState 需要持有沙箱会话引用后此处调用真实执行逻辑。
async fn execute_operation(
    operation: OperationRequest,
    state: &ConnectionState,
) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
    let start = std::time::Instant::now();

    info!(
        "执行操作请求: session={}, op_type={:?}, op_id={}",
        state.session_id, operation.operation_type, operation.operation_id
    );

    // 沙箱会话池尚未集成到 WebSocket 连接状态。
    // 返回明确的未实现错误，而非硬编码延迟后假装成功。
    // 集成路径：在 ConnectionState 中添加 Arc<SandboxSession> 字段，
    // 然后调用 session.execute(operation).await
    let _execution_time_ms = start.elapsed().as_millis() as u64;

    Err(format!(
        "沙箱会话池尚未集成 (session={}, op={:?}): 请通过 SandboxSessionPool 建立会话后重试",
        state.session_id, operation.operation_type
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_operation_type() {
        assert!(matches!(
            parse_operation_type("http_request"),
            OperationType::HttpRequest
        ));
        assert!(matches!(
            parse_operation_type("custom"),
            OperationType::Custom
        ));
        assert!(matches!(
            parse_operation_type("unknown"),
            OperationType::Custom
        ));
    }

    #[test]
    fn test_websocket_config_default() {
        let config = WebSocketConfig::default();
        assert_eq!(config.heartbeat_interval, 30);
        assert_eq!(config.operation_timeout, 60);
        assert_eq!(config.connection_timeout, 300);
    }

    #[test]
    fn test_client_message_deserialize() {
        let json = r#"{
            "type": "execute",
            "operation_id": "test-op-123",
            "operation_type": "navigate",
            "description": "Navigate to page",
            "parameters": {"url": "https://example.com"}
        }"#;

        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Execute {
                operation_id,
                operation_type,
                description,
                parameters,
            } => {
                assert_eq!(operation_id, Some("test-op-123".to_string()));
                assert_eq!(operation_type, "navigate");
                assert_eq!(description, "Navigate to page");
                assert!(parameters.contains_key("url"));
            }
            _ => panic!("Expected Execute message"),
        }
    }

    #[test]
    fn test_server_message_serialize() {
        let msg = ServerMessage::Connected {
            session_id: "test-session".to_string(),
            connected_at: "2024-01-01T00:00:00Z".to_string(),
            heartbeat_interval: 30,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("connected"));
        assert!(json.contains("test-session"));
    }
}
