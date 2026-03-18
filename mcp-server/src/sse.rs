//! MCP Server SSE Transport 实现
//!
//! 提供 Server-Sent Events (SSE) 传输层，支持与 OpenClaw Agent 的远程连接。

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error, info, warn};

use crate::handlers::ToolHandler;
use crate::auth::TokenClaims;

// 重新导出 message_queue 模块
pub use crate::message_queue;

/// 心跳间隔 (秒)
pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// 消息队列容量
pub const MESSAGE_QUEUE_CAPACITY: usize = 100;

/// Session 空闲超时 (5 分钟)
pub const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// SSE 消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum SseMessage {
    /// Endpoint 事件 - 连接建立后首先发送
    #[serde(rename = "endpoint")]
    Endpoint {
        /// 消息端点路径
        endpoint: String,
        /// Session Token
        token: String,
    },
    /// MCP 消息
    #[serde(rename = "message")]
    Message {
        /// MCP JSON-RPC 内容
        data: serde_json::Value,
    },
    /// 错误事件
    #[serde(rename = "error")]
    Error {
        /// 错误代码
        code: String,
        /// 错误消息
        message: String,
        /// 建议重试时间 (毫秒)
        retry: Option<u64>,
    },
    /// 心跳
    #[serde(rename = "connected")]
    Connected,
}

/// SSE 查询参数
#[derive(Debug, Deserialize)]
pub struct SseQueryParams {
    /// Session ID
    pub session_id: String,
}

/// Message 查询参数
#[derive(Debug, Deserialize)]
pub struct MessageQueryParams {
    /// Session ID
    pub session_id: String,
}

/// Session 状态
pub struct SessionState {
    /// Session ID
    pub id: String,
    /// Token Claims
    pub claims: TokenClaims,
    /// 待发送消息发送器
    pub tx: mpsc::Sender<SseMessage>,
    /// 接收客户端消息通道
    pub msg_tx: mpsc::Sender<McpRequest>,
    /// 最后活跃时间
    pub last_activity: Instant,
    /// SSE 连接是否活跃
    pub sse_connected: bool,
}

/// MCP 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    /// 请求 ID
    pub id: String,
    /// Session ID
    pub session_id: String,
    /// MCP JSON-RPC 内容
    pub payload: serde_json::Value,
}

/// Session 管理器
pub struct SessionManager {
    /// 活动 Session 映射
    sessions: tokio::sync::RwLock<std::collections::HashMap<String, Arc<SessionState>>>,
}

impl SessionManager {
    /// 创建新的 Session 管理器
    pub fn new() -> Self {
        Self {
            sessions: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// 注册新 Session
    pub async fn register(
        &self,
        session_id: String,
        tx: mpsc::Sender<SseMessage>,
        msg_tx: mpsc::Sender<McpRequest>,
        claims: TokenClaims,
    ) -> Result<(), SseError> {
        let mut sessions = self.sessions.write().await;

        // 检查是否已存在活跃 Session
        if let Some(existing) = sessions.get(&session_id) {
            if existing.sse_connected {
                return Err(SseError::SessionAlreadyConnected);
            }
        }

        let session = Arc::new(SessionState {
            id: session_id.clone(),
            claims,
            tx,
            msg_tx,
            last_activity: Instant::now(),
            sse_connected: true,
        });

        sessions.insert(session_id, session);
        Ok(())
    }

    /// 获取 Session
    pub async fn get_session(&self, session_id: &str) -> Option<Arc<SessionState>> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// 移除 Session
    pub async fn remove_session(&self, session_id: &str) -> Option<Arc<SessionState>> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id)
    }

    /// 清理过期 Session
    pub async fn cleanup_expired(&self) -> Vec<String> {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| now.duration_since(s.last_activity) > SESSION_IDLE_TIMEOUT)
            .map(|(id, _)| id.clone())
            .collect();

        for id in &expired {
            sessions.remove(id);
        }

        expired
    }

    /// 获取活跃 Session 数量
    pub async fn active_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// SSE 错误
#[derive(Debug, thiserror::Error)]
pub enum SseError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Session already connected")]
    SessionAlreadyConnected,

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Message queue full")]
    QueueFull,

    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    #[error("Connection lost")]
    ConnectionLost,

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for SseError {
    fn into_response(self) -> Response {
        use axum::http::StatusCode;
        let (status, body) = match self {
            SseError::SessionNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            SseError::SessionAlreadyConnected => (StatusCode::CONFLICT, self.to_string()),
            SseError::AuthFailed(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            SseError::TokenExpired => (StatusCode::UNAUTHORIZED, self.to_string()),
            SseError::QueueFull => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
            SseError::InvalidMessage(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            SseError::ConnectionLost => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            SseError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, body).into_response()
    }
}

/// SSE 应用状态
#[derive(Clone)]
pub struct SseAppState {
    /// Session 管理器
    pub sessions: Arc<SessionManager>,
    /// Tool Handler
    pub handler: ToolHandler,
}

/// 创建 SSE Router
pub fn create_sse_router(state: SseAppState) -> Router {
    Router::new()
        .route("/sse", get(sse_handler))
        .route("/message", post(message_handler))
        .route("/health", get(health_handler))
        .with_state(state)
}

/// SSE 连接处理器
pub async fn sse_handler(
    Query(params): Query<SseQueryParams>,
    headers: HeaderMap,
    State(state): State<SseAppState>,
) -> Result<SseResponse, SseError> {
    debug!("SSE connection request for session: {}", params.session_id);

    // 提取并验证 Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(SseError::AuthFailed("Missing authorization header".to_string()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(SseError::AuthFailed("Invalid token format".to_string()))?;

    // TODO: 实际验证 Token (当前简化处理，仅做演示)
    // 开发环境接受任何有效的 base64 token
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let _ = URL_SAFE_NO_PAD.decode(token)
        .unwrap_or_else(|_| Vec::new());

    // 创建消息通道
    let (tx, rx) = mpsc::channel(MESSAGE_QUEUE_CAPACITY);
    let (msg_tx, _msg_rx) = mpsc::channel(MESSAGE_QUEUE_CAPACITY);

    // 创建简化的 claims (开发环境)
    let claims = TokenClaims {
        iss: "credbridge-mcp".to_string(),
        sub: "dev_user".to_string(),
        aud: "mcp-agent".to_string(),
        exp: 9999999999,
        iat: 1000000000,
        jti: uuid::Uuid::new_v4().to_string(),
        sid: params.session_id.clone(),
        scp: vec!["mcp:connect".to_string()],
    };

    // 注册 Session
    state
        .sessions
        .register(params.session_id.clone(), tx, msg_tx, claims)
        .await?;

    info!("SSE session registered: {}", params.session_id);

    // 生成 Session Token
    let session_token = base64_encode(&params.session_id);

    // 创建 SSE 流
    let rx_stream = ReceiverStream::new(rx);
    let stream = create_sse_stream(rx_stream, session_token);

    Ok(SseResponse::new(stream.filter_map(|r| async move {
        r.ok()
    })))
}

fn base64_encode(s: &str) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(s.as_bytes())
}

/// 创建 SSE 流
fn create_sse_stream(
    rx: ReceiverStream<SseMessage>,
    session_token: String,
) -> impl Stream<Item = Result<SseMessage, SseError>> + Send + 'static {
    // 首先发送 endpoint 事件
    let endpoint_msg = SseMessage::Endpoint {
        endpoint: "/message".to_string(),
        token: session_token,
    };

    // 心跳流
    let heartbeat = tokio_stream::wrappers::IntervalStream::new(
        tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS))
    )
    .map(|_| Ok::<_, SseError>(SseMessage::Connected));

    // 合并流
    futures::stream::once(futures::future::ok(endpoint_msg))
        .chain(rx.map(Ok))
        .chain(heartbeat)
}

/// 消息处理器
pub async fn message_handler(
    Query(params): Query<MessageQueryParams>,
    State(_state): State<SseAppState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<MessageResponse> {
    debug!("Message received for session: {}", params.session_id);

    // 提取消息 ID
    let message_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    info!("Message received: {} for session: {}", message_id, params.session_id);

    // TODO: 处理 MCP 请求并发送到 ToolHandler

    Json(MessageResponse {
        accepted: true,
        message_id,
    })
}

/// 健康检查处理器
pub async fn health_handler(
    State(state): State<SseAppState>,
) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        active_sessions: state.sessions.active_count().await,
        uptime_seconds: 0,
    })
}

/// 消息响应
#[derive(Debug, Serialize, Deserialize)]
pub struct MessageResponse {
    pub accepted: bool,
    pub message_id: String,
}

/// 健康响应
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub active_sessions: usize,
    pub uptime_seconds: u64,
}

/// SSE 响应封装
pub struct SseResponse {
    stream: ReceiverStream<Result<String, std::convert::Infallible>>,
}

impl SseResponse {
    pub fn new(
        stream: impl Stream<Item = SseMessage> + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel(100);

        // 后台任务：转换流
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = std::pin::pin!(stream);
            while let Some(msg) = stream.next().await {
                let formatted = match msg {
                    SseMessage::Endpoint { endpoint, token } => {
                        format!(
                            "event: endpoint\ndata: {{\"endpoint\":\"{}\",\"token\":\"{}\"}}\n\n",
                            endpoint, token
                        )
                    }
                    SseMessage::Message { data } => {
                        format!("event: message\ndata: {}\n\n", data)
                    }
                    SseMessage::Error { code, message, retry } => {
                        let retry_str = retry
                            .map(|r| format!(",\"retry\":{}", r))
                            .unwrap_or_default();
                        format!(
                            "event: error\ndata: {{\"code\":\"{}\",\"message\":\"{}\"{}}}\n\n",
                            code, message, retry_str
                        )
                    }
                    SseMessage::Connected => {
                        // 心跳使用 SSE comment
                        ":ping\n\n".to_string()
                    }
                };
                if tx.send(Ok(formatted)).await.is_err() {
                    break;
                }
            }
        });

        Self {
            stream: ReceiverStream::new(rx),
        }
    }
}

impl IntoResponse for SseResponse {
    fn into_response(self) -> Response {
        use axum::body::Body;
        use axum::http::HeaderValue;

        let body = Body::from_stream(self.stream);

        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("text/event-stream"));
        headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
        headers.insert("Connection", HeaderValue::from_static("keep-alive"));
        headers.insert("X-Accel-Buffering", HeaderValue::from_static("no"));

        (headers, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_message_serialization() {
        let msg = SseMessage::Endpoint {
            endpoint: "/message".to_string(),
            token: "test_token".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("endpoint"));
        assert!(json.contains("token"));
    }

    #[test]
    fn test_message_response() {
        let response = MessageResponse {
            accepted: true,
            message_id: "msg_789".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("accepted"));
        assert!(json.contains("msg_789"));
    }

    #[tokio::test]
    async fn test_session_manager_basic() {
        let manager = SessionManager::new();

        let (tx, _rx) = mpsc::channel(10);
        let (msg_tx, _msg_rx) = mpsc::channel(10);

        let claims = TokenClaims {
            iss: "test".to_string(),
            sub: "user".to_string(),
            aud: "agent".to_string(),
            exp: 9999999999,
            iat: 1000000000,
            jti: uuid::Uuid::new_v4().to_string(),
            sid: "session_1".to_string(),
            scp: vec!["mcp:connect".to_string()],
        };

        assert!(manager.register("session_1".to_string(), tx, msg_tx, claims).await.is_ok());
        assert_eq!(manager.active_count().await, 1);

        let session = manager.get_session("session_1").await;
        assert!(session.is_some());
    }
}
