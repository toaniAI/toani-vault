//! 凭证命名空间隔离
//!
//! 实现凭证命名空间隔离，确保不同沙箱之间的凭证无法互相访问

use crate::tee::sandbox::error::SecurityError;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// 凭证命名空间
///
/// 每个沙箱会话拥有独立的凭证命名空间，
/// 凭证句柄只能在创建它的命名空间内解析
#[derive(Debug, Clone)]
pub struct CredentialNamespace {
    /// 命名空间 ID（对应沙箱 ID）
    pub namespace_id: Uuid,
    /// 凭证存储（句柄 -> 凭证内容）
    credentials: Arc<RwLock<HashMap<String, StoredCredential>>>,
}

/// 存储的凭证
#[derive(Debug, Clone)]
struct StoredCredential {
    /// 凭证数据（加密存储）
    data: Vec<u8>,
    /// 元数据
    metadata: CredentialMetadata,
}

/// 凭证元数据
#[derive(Debug, Clone)]
pub struct CredentialMetadata {
    /// 凭证 ID
    pub credential_id: Uuid,
    /// 凭证类型
    pub credential_type: String,
    /// 创建时间
    pub created_at: time::OffsetDateTime,
    /// 过期时间
    pub expires_at: Option<time::OffsetDateTime>,
}

/// 凭证句柄
///
/// 用于在沙箱内引用凭证，不包含实际凭证内容
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialHandle {
    /// 句柄 ID
    pub handle_id: String,
    /// 命名空间 ID
    pub namespace_id: Uuid,
    /// 凭证 ID
    pub credential_id: Uuid,
}

impl CredentialNamespace {
    /// 创建新的凭证命名空间
    pub fn new(namespace_id: Uuid) -> Self {
        Self {
            namespace_id,
            credentials: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 存储凭证
    ///
    /// 返回凭证句柄，不包含实际凭证内容
    pub async fn store(
        &self,
        credential_id: Uuid,
        credential_type: &str,
        encrypted_data: Vec<u8>,
    ) -> Result<CredentialHandle, SecurityError> {
        let handle_id = format!("{}_{}", self.namespace_id, credential_id);

        let stored = StoredCredential {
            data: encrypted_data,
            metadata: CredentialMetadata {
                credential_id,
                credential_type: credential_type.to_string(),
                created_at: time::OffsetDateTime::now_utc(),
                expires_at: None,
            },
        };

        self.credentials
            .write()
            .await
            .insert(handle_id.clone(), stored);

        info!(
            "Credential {} stored in namespace {}",
            credential_id, self.namespace_id
        );

        Ok(CredentialHandle {
            handle_id,
            namespace_id: self.namespace_id,
            credential_id,
        })
    }

    /// 获取凭证
    ///
    /// 只能访问本命名空间内的凭证
    pub async fn get(&self, handle_id: &str) -> Result<Option<Vec<u8>>, SecurityError> {
        // 验证句柄属于本命名空间
        if !self.validate_handle(handle_id) {
            warn!(
                "Attempt to access foreign credential handle {} from namespace {}",
                handle_id, self.namespace_id
            );
            return Err(SecurityError::CredentialIsolation(
                "Cross-namespace credential access denied".to_string(),
            ));
        }

        let credentials = self.credentials.read().await;
        Ok(credentials.get(handle_id).map(|c| c.data.clone()))
    }

    /// 删除凭证
    pub async fn remove(&self, handle_id: &str) -> Result<bool, SecurityError> {
        if !self.validate_handle(handle_id) {
            return Err(SecurityError::CredentialIsolation(
                "Cross-namespace credential access denied".to_string(),
            ));
        }

        let removed = self.credentials.write().await.remove(handle_id).is_some();

        if removed {
            debug!(
                "Credential {} removed from namespace {}",
                handle_id, self.namespace_id
            );
        }

        Ok(removed)
    }

    /// 验证句柄是否属于本命名空间
    fn validate_handle(&self, handle_id: &str) -> bool {
        handle_id.starts_with(&self.namespace_id.to_string())
    }

    /// 列出命名空间中的所有凭证
    pub async fn list(&self) -> Vec<CredentialMetadata> {
        self.credentials
            .read()
            .await
            .values()
            .map(|c| c.metadata.clone())
            .collect()
    }

    /// 清空命名空间中的所有凭证
    pub async fn clear(&self) {
        let count = self.credentials.read().await.len();
        self.credentials.write().await.clear();

        info!(
            "Cleared {} credentials from namespace {}",
            count, self.namespace_id
        );
    }

    /// 获取命名空间中的凭证数量
    pub async fn count(&self) -> usize {
        self.credentials.read().await.len()
    }
}

/// 凭证命名空间管理器
///
/// 管理多个凭证命名空间，提供隔离保障
pub struct CredentialNamespaceManager {
    /// 命名空间映射
    namespaces: Arc<RwLock<HashMap<Uuid, CredentialNamespace>>>,
}

impl CredentialNamespaceManager {
    /// 创建新的管理器
    pub fn new() -> Self {
        Self {
            namespaces: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建新的命名空间
    pub async fn create_namespace(&self, namespace_id: Uuid) -> CredentialNamespace {
        let namespace = CredentialNamespace::new(namespace_id);

        self.namespaces
            .write()
            .await
            .insert(namespace_id, namespace.clone());

        info!("Created credential namespace {}", namespace_id);

        namespace
    }

    /// 获取命名空间
    pub async fn get_namespace(&self, namespace_id: Uuid) -> Option<CredentialNamespace> {
        self.namespaces.read().await.get(&namespace_id).cloned()
    }

    /// 删除命名空间
    pub async fn remove_namespace(&self, namespace_id: Uuid) {
        if let Some(namespace) = self.namespaces.write().await.remove(&namespace_id) {
            namespace.clear().await;
            info!("Removed credential namespace {}", namespace_id);
        }
    }

    /// 获取所有命名空间 ID
    pub async fn list_namespaces(&self) -> Vec<Uuid> {
        self.namespaces.read().await.keys().copied().collect()
    }

    /// 验证跨命名空间访问
    ///
    /// 如果访问被允许返回 true，否则返回 false
    pub async fn validate_cross_access(&self, source_namespace: Uuid, target_handle: &str) -> bool {
        target_handle.starts_with(&source_namespace.to_string())
    }
}

impl Default for CredentialNamespaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_credential_namespace_store_and_get() {
        let namespace_id = Uuid::new_v4();
        let ns = CredentialNamespace::new(namespace_id);

        let credential_id = Uuid::new_v4();
        let encrypted_data = b"encrypted credential data".to_vec();

        // 存储凭证
        let handle = ns
            .store(credential_id, "password", encrypted_data.clone())
            .await
            .unwrap();

        assert_eq!(handle.namespace_id, namespace_id);
        assert_eq!(handle.credential_id, credential_id);

        // 获取凭证
        let retrieved = ns.get(&handle.handle_id).await.unwrap();
        assert_eq!(retrieved, Some(encrypted_data));
    }

    #[tokio::test]
    async fn test_cross_namespace_access_denied() {
        let ns1_id = Uuid::new_v4();
        let ns2_id = Uuid::new_v4();

        let ns1 = CredentialNamespace::new(ns1_id);
        let ns2 = CredentialNamespace::new(ns2_id);

        let credential_id = Uuid::new_v4();
        let encrypted_data = b"encrypted credential data".to_vec();

        // 在 ns1 中存储凭证
        let handle = ns1
            .store(credential_id, "password", encrypted_data)
            .await
            .unwrap();

        // ns2 尝试访问应该失败
        let result = ns2.get(&handle.handle_id).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SecurityError::CredentialIsolation(_)
        ));
    }

    #[tokio::test]
    async fn test_namespace_manager() {
        let manager = CredentialNamespaceManager::new();
        let ns_id = Uuid::new_v4();

        // 创建命名空间
        let ns = manager.create_namespace(ns_id).await;
        assert_eq!(ns.namespace_id, ns_id);

        // 获取命名空间
        let retrieved = manager.get_namespace(ns_id).await;
        assert!(retrieved.is_some());

        // 列出命名空间
        let namespaces = manager.list_namespaces().await;
        assert_eq!(namespaces.len(), 1);

        // 删除命名空间
        manager.remove_namespace(ns_id).await;
        let retrieved = manager.get_namespace(ns_id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_validate_cross_access() {
        let manager = CredentialNamespaceManager::new();
        let ns_id = Uuid::new_v4();

        let handle_id = format!("{}_{}", ns_id, Uuid::new_v4());

        // 同命名空间访问应该被允许
        assert!(manager.validate_cross_access(ns_id, &handle_id).await);

        // 跨命名空间访问应该被拒绝
        let other_ns_id = Uuid::new_v4();
        assert!(!manager.validate_cross_access(other_ns_id, &handle_id).await);
    }
}
