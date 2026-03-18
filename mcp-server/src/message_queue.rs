//! MCP 消息队列实现
//!
//! 提供 Session 级别的消息队列管理，支持消息的可靠传递。
//!
//! ## 架构设计
//!
//! - 每个 Session 维护独立的 mpsc 队列
//! - 支持消息确认机制
//! - 可选的消息持久化
//! - 背压处理 (队列满时拒绝)
//!
//! ## 消息类型
//!
//! - **McpRequest**: 客户端 → 服务端
//! - **McpResponse**: 服务端 → 客户端
//! - **McpNotification**: 单向通知

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// 消息队列默认容量
pub const DEFAULT_QUEUE_CAPACITY: usize = 100;

/// 消息重试次数上限
pub const MAX_RETRY_COUNT: u32 = 3;

/// 消息处理超时
pub const MESSAGE_PROCESSING_TIMEOUT: Duration = Duration::from_secs(30);

/// MCP 消息类型
#[derive(Debug, Clone)]
pub enum McpMessage {
    /// MCP 请求
    Request {
        id: String,
        method: String,
        params: Option<serde_json::Value>,
    },
    /// MCP 响应
    Response {
        id: String,
        result: Option<serde_json::Value>,
        error: Option<McpError>,
    },
    /// MCP 通知 (无 ID)
    Notification {
        method: String,
        params: Option<serde_json::Value>,
    },
}

impl Serialize for McpMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("jsonrpc", "2.0")?;

        match self {
            McpMessage::Request { id, method, params } => {
                map.serialize_entry("id", id)?;
                map.serialize_entry("method", method)?;
                if let Some(p) = params {
                    map.serialize_entry("params", p)?;
                }
            }
            McpMessage::Response { id, result, error } => {
                map.serialize_entry("id", id)?;
                if let Some(r) = result {
                    map.serialize_entry("result", r)?;
                }
                if let Some(e) = error {
                    map.serialize_entry("error", e)?;
                }
            }
            McpMessage::Notification { method, params } => {
                map.serialize_entry("method", method)?;
                if let Some(p) = params {
                    map.serialize_entry("params", p)?;
                }
            }
        }

        map.end()
    }
}

impl<'de> Deserialize<'de> for McpMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct McpMessageVisitor;

        impl<'de> Visitor<'de> for McpMessageVisitor {
            type Value = McpMessage;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON-RPC 2.0 message object")
            }

            fn visit_map<V>(self, mut map: V) -> Result<McpMessage, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut jsonrpc: Option<String> = None;
                let mut id: Option<String> = None;
                let mut method: Option<String> = None;
                let mut params: Option<serde_json::Value> = None;
                let mut result: Option<serde_json::Value> = None;
                let mut error: Option<McpError> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "jsonrpc" => {
                            jsonrpc = Some(map.next_value()?);
                        }
                        "id" => {
                            id = Some(map.next_value()?);
                        }
                        "method" => {
                            method = Some(map.next_value()?);
                        }
                        "params" => {
                            params = Some(map.next_value()?);
                        }
                        "result" => {
                            result = Some(map.next_value()?);
                        }
                        "error" => {
                            error = Some(map.next_value()?);
                        }
                        _ => {
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                // 验证 jsonrpc 版本
                match jsonrpc.as_deref() {
                    Some("2.0") => {}
                    Some(v) => {
                        return Err(de::Error::custom(format!(
                            "Unsupported jsonrpc version: {}",
                            v
                        )));
                    }
                    None => {
                        return Err(de::Error::custom("Missing jsonrpc field"));
                    }
                }

                // 判断消息类型
                if method.is_some() {
                    if let Some(id) = id {
                        // 请求
                        Ok(McpMessage::Request {
                            id,
                            method: method.unwrap(),
                            params,
                        })
                    } else {
                        // 通知
                        Ok(McpMessage::Notification {
                            method: method.unwrap(),
                            params,
                        })
                    }
                } else if let Some(id) = id {
                    // 响应
                    Ok(McpMessage::Response { id, result, error })
                } else {
                    Err(de::Error::custom(
                        "Invalid message: must have either 'method' or 'id' field",
                    ))
                }
            }
        }

        deserializer.deserialize_map(McpMessageVisitor)
    }
}

impl McpMessage {
    /// 获取消息 ID (如果有)
    pub fn id(&self) -> Option<&str> {
        match self {
            McpMessage::Request { id, .. } => Some(id),
            McpMessage::Response { id, .. } => Some(id),
            McpMessage::Notification { .. } => None,
        }
    }

    /// 判断是否为请求
    pub fn is_request(&self) -> bool {
        matches!(self, McpMessage::Request { .. })
    }

    /// 判断是否为响应
    pub fn is_response(&self) -> bool {
        matches!(self, McpMessage::Response { .. })
    }

    /// 判断是否为通知
    pub fn is_notification(&self) -> bool {
        matches!(self, McpMessage::Notification { .. })
    }

    /// 创建请求
    pub fn request(id: String, method: String, params: Option<serde_json::Value>) -> Self {
        McpMessage::Request { id, method, params }
    }

    /// 创建成功响应
    pub fn response(id: String, result: serde_json::Value) -> Self {
        McpMessage::Response {
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 创建错误响应
    pub fn error_response(id: String, error: McpError) -> Self {
        McpMessage::Response {
            id,
            result: None,
            error: Some(error),
        }
    }

    /// 创建通知
    pub fn notification(method: String, params: Option<serde_json::Value>) -> Self {
        McpMessage::Notification { method, params }
    }

    /// 转换为 JSON Value
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|e| {
            error!("Failed to serialize McpMessage: {}", e);
            serde_json::json!({"error": "Serialization failed"})
        })
    }

    /// 从 JSON Value 解析
    pub fn from_json(value: serde_json::Value) -> Result<Self, McpMessageError> {
        // 检查 jsonrpc 版本
        let version = value.get("jsonrpc")
            .and_then(|v| v.as_str())
            .ok_or(McpMessageError::InvalidFormat("Missing jsonrpc field".to_string()))?;

        if version != "2.0" {
            return Err(McpMessageError::InvalidFormat(
                format!("Unsupported jsonrpc version: {}", version)
            ));
        }

        // 判断消息类型
        let has_id = value.get("id").is_some();
        let has_error = value.get("error").is_some();

        if has_id && !has_error {
            // 可能是请求或成功响应
            if value.get("method").is_some() {
                // 请求
                Ok(McpMessage::Request {
                    id: value.get("id").unwrap().as_str().unwrap().to_string(),
                    method: value.get("method").unwrap().as_str().unwrap().to_string(),
                    params: value.get("params").cloned(),
                })
            } else {
                // 成功响应
                Ok(McpMessage::Response {
                    id: value.get("id").unwrap().as_str().unwrap().to_string(),
                    result: value.get("result").cloned(),
                    error: None,
                })
            }
        } else if has_id && has_error {
            // 错误响应
            Ok(McpMessage::Response {
                id: value.get("id").unwrap().as_str().unwrap().to_string(),
                result: None,
                error: value.get("error").and_then(|e| {
                    serde_json::from_value(e.clone()).ok()
                }),
            })
        } else {
            // 通知
            Ok(McpMessage::Notification {
                method: value.get("method")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .ok_or_else(|| McpMessageError::InvalidFormat("Missing method field".to_string()))?,
                params: value.get("params").cloned(),
            })
        }
    }
}

/// MCP 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl McpError {
    /// 创建解析错误
    pub fn parse_error(data: Option<serde_json::Value>) -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data,
        }
    }

    /// 创建无效请求错误
    pub fn invalid_request(data: Option<serde_json::Value>) -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
            data,
        }
    }

    /// 创建方法不存在错误
    pub fn method_not_found(data: Option<serde_json::Value>) -> Self {
        Self {
            code: -32601,
            message: "Method not found".to_string(),
            data,
        }
    }

    /// 创建无效参数错误
    pub fn invalid_params(data: Option<serde_json::Value>) -> Self {
        Self {
            code: -32602,
            message: "Invalid params".to_string(),
            data,
        }
    }

    /// 创建内部错误
    pub fn internal_error(data: Option<serde_json::Value>) -> Self {
        Self {
            code: -32603,
            message: "Internal error".to_string(),
            data,
        }
    }

    /// 创建工具错误
    pub fn tool_error(code: i32, message: String) -> Self {
        Self {
            code,
            message,
            data: None,
        }
    }
}

/// MCP 消息错误
#[derive(Debug, thiserror::Error)]
pub enum McpMessageError {
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Queue error: {0}")]
    QueueError(String),
}

/// 消息确认状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AckStatus {
    /// 消息已接收
    Received,
    /// 处理中
    Processing,
    /// 处理完成
    Completed,
    /// 处理失败
    Failed { error: String },
    /// 已重试
    Retrying { attempt: u32 },
}

/// 消息确认
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAck {
    /// 消息 ID
    pub message_id: String,
    /// Session ID
    pub session_id: String,
    /// 确认状态
    pub status: AckStatus,
    /// 时间戳
    pub timestamp: u64,
}

impl MessageAck {
    pub fn new(message_id: String, session_id: String, status: AckStatus) -> Self {
        Self {
            message_id,
            session_id,
            status,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn received(message_id: String, session_id: String) -> Self {
        Self::new(message_id, session_id, AckStatus::Received)
    }

    pub fn processing(message_id: String, session_id: String) -> Self {
        Self::new(message_id, session_id, AckStatus::Processing)
    }

    pub fn completed(message_id: String, session_id: String) -> Self {
        Self::new(message_id, session_id, AckStatus::Completed)
    }

    pub fn failed(message_id: String, session_id: String, error: String) -> Self {
        Self::new(message_id, session_id, AckStatus::Failed { error })
    }
}

use std::time::{SystemTime, UNIX_EPOCH};

/// 队列消息包装器
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// 消息内容
    pub message: McpMessage,
    /// 入队时间
    pub enqueued_at: Instant,
    /// 重试次数
    pub retry_count: u32,
    /// 消息确认
    pub ack: Option<MessageAck>,
}

impl QueuedMessage {
    pub fn new(message: McpMessage) -> Self {
        Self {
            message,
            enqueued_at: Instant::now(),
            retry_count: 0,
            ack: None,
        }
    }

    /// 检查是否超时
    pub fn is_timeout(&self) -> bool {
        self.enqueued_at.elapsed() > MESSAGE_PROCESSING_TIMEOUT
    }

    /// 检查是否可以重试
    pub fn can_retry(&self) -> bool {
        self.retry_count < MAX_RETRY_COUNT
    }

    /// 标记为已重试
    pub fn retry(&mut self) {
        self.retry_count += 1;
        self.enqueued_at = Instant::now();
    }
}

/// 消息队列 trait
#[async_trait::async_trait]
pub trait MessageQueue: Send + Sync {
    /// 入队消息
    async fn enqueue(&self, session_id: &str, message: McpMessage) -> Result<(), McpMessageError>;

    /// 出队消息
    async fn dequeue(&self, session_id: &str) -> Option<McpMessage>;

    /// 确认消息
    async fn ack(&self, session_id: &str, message_id: &str) -> Result<(), McpMessageError>;

    /// 获取队列长度
    async fn len(&self, session_id: &str) -> usize;

    /// 检查队列是否为空
    async fn is_empty(&self, session_id: &str) -> bool {
        self.len(session_id).await == 0
    }

    /// 清理过期消息
    async fn cleanup_expired(&self);
}

/// 内存消息队列实现
pub struct InMemoryMessageQueue {
    /// 队列容量
    capacity: usize,
    /// Session 队列映射
    queues: RwLock<std::collections::HashMap<String, Arc<Mutex<VecDeque<QueuedMessage>>>>>,
    /// 确认记录
    acks: RwLock<std::collections::HashMap<String, MessageAck>>,
}

impl InMemoryMessageQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            queues: RwLock::new(std::collections::HashMap::new()),
            acks: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// 获取或创建 Session 队列
    async fn get_or_create_queue(
        &self,
        session_id: &str,
    ) -> Arc<Mutex<VecDeque<QueuedMessage>>> {
        let mut queues = self.queues.write().await;

        queues
            .entry(session_id.to_string())
            .or_insert_with(|| {
                Arc::new(Mutex::new(VecDeque::with_capacity(self.capacity)))
            })
            .clone()
    }

    /// 移除 Session 队列
    pub async fn remove_queue(&self, session_id: &str) {
        let mut queues = self.queues.write().await;
        queues.remove(session_id);

        let mut acks = self.acks.write().await;
        acks.retain(|k, _| k != session_id);
    }

    /// 获取所有 Session ID
    pub async fn session_ids(&self) -> Vec<String> {
        let queues = self.queues.read().await;
        queues.keys().cloned().collect()
    }
}

#[async_trait::async_trait]
impl MessageQueue for InMemoryMessageQueue {
    async fn enqueue(&self, session_id: &str, message: McpMessage) -> Result<(), McpMessageError> {
        let queue = self.get_or_create_queue(session_id).await;
        let mut queue = queue.lock().await;

        // 检查队列容量
        if queue.len() >= self.capacity {
            warn!("Message queue full for session: {}", session_id);
            return Err(McpMessageError::QueueError("Queue full".to_string()));
        }

        let queued_msg = QueuedMessage::new(message);
        queue.push_back(queued_msg);

        debug!("Message enqueued for session: {}", session_id);
        Ok(())
    }

    async fn dequeue(&self, session_id: &str) -> Option<McpMessage> {
        let queue = self.get_or_create_queue(session_id).await;
        let mut queue = queue.lock().await;

        // 查找第一个未确认的消息
        for item in queue.iter_mut() {
            if item.ack.is_none() || matches!(item.ack.as_ref().unwrap().status, AckStatus::Failed { .. }) {
                // 更新确认为已接收
                item.ack = Some(MessageAck::received(
                    item.message.id().unwrap_or("unknown").to_string(),
                    session_id.to_string(),
                ));

                return Some(item.message.clone());
            }
        }

        None
    }

    async fn ack(&self, session_id: &str, message_id: &str) -> Result<(), McpMessageError> {
        let queue = self.get_or_create_queue(session_id).await;
        let mut queue = queue.lock().await;

        // 查找并确认消息
        for item in queue.iter_mut() {
            if item.message.id() == Some(message_id) {
                item.ack = Some(MessageAck::completed(
                    message_id.to_string(),
                    session_id.to_string(),
                ));

                // 记录确认
                let mut acks = self.acks.write().await;
                acks.insert(
                    format!("{}:{}", session_id, message_id),
                    MessageAck::completed(message_id.to_string(), session_id.to_string()),
                );

                debug!("Message acknowledged: {} for session: {}", message_id, session_id);
                return Ok(());
            }
        }

        warn!("Message not found for ack: {} in session: {}", message_id, session_id);
        Err(McpMessageError::QueueError("Message not found".to_string()))
    }

    async fn len(&self, session_id: &str) -> usize {
        let queue = self.get_or_create_queue(session_id).await;
        let queue = queue.lock().await;
        queue.len()
    }

    async fn cleanup_expired(&self) {
        let mut queues = self.queues.write().await;
        let expired_keys: Vec<String> = queues
            .iter()
            .filter(|(_, q)| {
                let q = q.blocking_lock();
                q.iter().any(|item| item.is_timeout() && !item.can_retry())
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in &expired_keys {
            info!("Cleaning up expired messages for session: {}", key);
            let mut queue = queues.get(key).unwrap().lock().await;
            queue.retain(|item| !item.is_timeout() || item.can_retry());
        }

        info!("Cleanup removed {} expired session queues", expired_keys.len());
    }
}

/// 消息持久化 trait
#[async_trait::async_trait]
pub trait MessagePersistence: Send + Sync {
    /// 持久化消息
    async fn store(&self, session_id: &str, message: &McpMessage) -> Result<(), McpMessageError>;

    /// 加载未发送的消息
    async fn load_pending(&self, session_id: &str) -> Result<Vec<McpMessage>, McpMessageError>;

    /// 标记消息为已发送
    async fn mark_sent(&self, session_id: &str, message_id: &str) -> Result<(), McpMessageError>;

    /// 删除消息
    async fn delete(&self, session_id: &str, message_id: &str) -> Result<(), McpMessageError>;
}

/// 无操作持久化实现 (默认)
pub struct NoOpPersistence;

#[async_trait::async_trait]
impl MessagePersistence for NoOpPersistence {
    async fn store(&self, _session_id: &str, _message: &McpMessage) -> Result<(), McpMessageError> {
        Ok(())
    }

    async fn load_pending(&self, _session_id: &str) -> Result<Vec<McpMessage>, McpMessageError> {
        Ok(Vec::new())
    }

    async fn mark_sent(&self, _session_id: &str, _message_id: &str) -> Result<(), McpMessageError> {
        Ok(())
    }

    async fn delete(&self, _session_id: &str, _message_id: &str) -> Result<(), McpMessageError> {
        Ok(())
    }
}

/// 消息队列管理器
pub struct MessageQueueManager {
    /// 消息队列
    queue: Arc<dyn MessageQueue>,
    /// 持久化 (可选)
    persistence: Option<Arc<dyn MessagePersistence>>,
}

impl MessageQueueManager {
    pub fn new(queue: Arc<dyn MessageQueue>) -> Self {
        Self {
            queue,
            persistence: None,
        }
    }

    pub fn with_persistence(mut self, persistence: Arc<dyn MessagePersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// 发送消息
    pub async fn send(&self, session_id: &str, message: McpMessage) -> Result<(), McpMessageError> {
        // 持久化 (如果配置了)
        if let Some(ref persistence) = self.persistence {
            persistence.store(session_id, &message).await?;
        }

        // 入队
        self.queue.enqueue(session_id, message).await?;
        Ok(())
    }

    /// 接收消息
    pub async fn receive(&self, session_id: &str) -> Option<McpMessage> {
        self.queue.dequeue(session_id).await
    }

    /// 确认消息
    pub async fn acknowledge(&self, session_id: &str, message_id: &str) -> Result<(), McpMessageError> {
        self.queue.ack(session_id, message_id).await?;

        // 删除持久化消息
        if let Some(ref persistence) = self.persistence {
            let _ = persistence.delete(session_id, message_id).await;
        }

        Ok(())
    }

    /// 获取队列长度
    pub async fn queue_length(&self, session_id: &str) -> usize {
        self.queue.len(session_id).await
    }

    /// 清理过期消息
    pub async fn cleanup(&self) {
        self.queue.cleanup_expired().await;
    }

    /// 获取内部队列引用
    pub fn inner_queue(&self) -> Arc<dyn MessageQueue> {
        self.queue.clone()
    }
}

/// 背压控制器
pub struct BackpressureController {
    /// 高水位标记
    high_watermark: usize,
    /// 低水位标记
    low_watermark: usize,
}

impl BackpressureController {
    pub fn new(high_watermark: usize, low_watermark: usize) -> Self {
        Self {
            high_watermark,
            low_watermark,
        }
    }

    pub fn default() -> Self {
        Self {
            high_watermark: 80,  // 80% 容量
            low_watermark: 20,   // 20% 容量
        }
    }

    /// 检查是否需要背压
    pub fn should_apply_backpressure(&self, current_size: usize, capacity: usize) -> bool {
        let usage_percent = (current_size * 100) / capacity;
        usage_percent >= self.high_watermark
    }

    /// 检查背压是否解除
    pub fn backpressure_cleared(&self, current_size: usize, capacity: usize) -> bool {
        let usage_percent = (current_size * 100) / capacity;
        usage_percent <= self.low_watermark
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mcp_message_creation() {
        let msg = McpMessage::request(
            "req_123".to_string(),
            "tools/call".to_string(),
            Some(json!({"name": "test"})),
        );

        assert!(msg.is_request());
        assert_eq!(msg.id(), Some("req_123"));
    }

    #[test]
    fn test_mcp_message_response() {
        let msg = McpMessage::response(
            "req_123".to_string(),
            json!({"result": "success"}),
        );

        assert!(msg.is_response());
        assert_eq!(msg.id(), Some("req_123"));
    }

    #[test]
    fn test_mcp_message_notification() {
        let msg = McpMessage::notification(
            "tools/list_changed".to_string(),
            None,
        );

        assert!(msg.is_notification());
        assert_eq!(msg.id(), None);
    }

    #[test]
    fn test_mcp_message_json_roundtrip() {
        let original = McpMessage::request(
            "req_123".to_string(),
            "tools/call".to_string(),
            Some(json!({"name": "test", "value": 42})),
        );

        let json = original.to_json();
        let restored = McpMessage::from_json(json).unwrap();

        assert_eq!(original.id(), restored.id());
        assert!(restored.is_request());
    }

    #[test]
    fn test_mcp_error_creation() {
        let error = McpError::invalid_request(Some(json!({"detail": "Missing field"})));

        assert_eq!(error.code, -32600);
        assert_eq!(error.message, "Invalid Request");
    }

    #[test]
    fn test_message_ack() {
        let ack = MessageAck::received("msg_123".to_string(), "session_456".to_string());

        assert_eq!(ack.message_id, "msg_123");
        assert_eq!(ack.session_id, "session_456");
        assert_eq!(ack.status, AckStatus::Received);
    }

    #[test]
    fn test_queued_message_timeout() {
        let msg = QueuedMessage::new(McpMessage::notification("test".to_string(), None));

        // 刚创建的消息不应该超时
        assert!(!msg.is_timeout());

        // 应该可以重试
        assert!(msg.can_retry());
    }

    #[tokio::test]
    async fn test_in_memory_queue_basic() {
        let queue = Arc::new(InMemoryMessageQueue::new(10));

        let msg = McpMessage::request(
            "req_1".to_string(),
            "test".to_string(),
            None,
        );

        // 入队
        assert!(queue.enqueue("session_1", msg.clone()).await.is_ok());

        // 检查长度
        assert_eq!(queue.len("session_1").await, 1);

        // 出队
        let dequeued = queue.dequeue("session_1").await;
        assert!(dequeued.is_some());

        // 确认
        assert!(queue.ack("session_1", "req_1").await.is_ok());
    }

    #[tokio::test]
    async fn test_in_memory_queue_capacity() {
        let queue = Arc::new(InMemoryMessageQueue::new(2));

        // 填满队列
        for i in 0..2 {
            let msg = McpMessage::request(format!("req_{}", i), "test".to_string(), None);
            assert!(queue.enqueue("session_1", msg).await.is_ok());
        }

        // 第 3 个应该失败
        let msg = McpMessage::request("req_overflow".to_string(), "test".to_string(), None);
        assert!(queue.enqueue("session_1", msg).await.is_err());
    }

    #[tokio::test]
    async fn test_message_queue_manager() {
        let queue = Arc::new(InMemoryMessageQueue::new(10));
        let manager = MessageQueueManager::new(queue);

        let msg = McpMessage::request(
            "req_1".to_string(),
            "test".to_string(),
            None,
        );

        // 发送
        assert!(manager.send("session_1", msg.clone()).await.is_ok());

        // 接收
        let received = manager.receive("session_1").await;
        assert!(received.is_some());

        // 确认
        assert!(manager.acknowledge("session_1", "req_1").await.is_ok());
    }

    #[test]
    fn test_backpressure_controller() {
        let controller = BackpressureController::default();

        // 80% 使用率应该触发背压
        assert!(controller.should_apply_backpressure(80, 100));

        // 50% 使用率不应该触发
        assert!(!controller.should_apply_backpressure(50, 100));

        // 20% 使用率应该解除背压
        assert!(controller.backpressure_cleared(20, 100));
    }
}
