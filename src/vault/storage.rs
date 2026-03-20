//! 凭证存储逻辑
//!
//! 实现多租户隔离存储：
//! - Schema-per-Tenant 架构
//! - 行级安全（RLS）
//! - 仅授权用户可访问其租户数据

use super::models::*;
use super::version::CredentialVersion;
use crate::models::CredentialMetadata;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// 存储后端 trait（抽象存储实现）
///
/// 支持多种存储后端：内存、PostgreSQL、HashiCorp Vault
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// 存储凭证条目
    fn store(&self, entry: &VaultEntry) -> Result<(), VaultError>;

    /// 根据 ID 获取凭证（完整数据）
    fn get(&self, credential_id: &CredentialId) -> Result<Option<VaultEntry>, VaultError>;

    /// 根据 ID 获取凭证元数据（不含加密载荷）
    fn get_metadata(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Option<CredentialMetadata>, VaultError>;

    /// 查询用户的所有凭证（仅元数据）
    fn query(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        filter: &CredentialFilter,
    ) -> Result<CredentialQueryResult, VaultError>;

    /// 删除凭证（软删除）
    fn delete(&self, credential_id: &CredentialId) -> Result<bool, VaultError>;

    /// 物理删除凭证（用于合规清理）
    fn purge(&self, credential_id: &CredentialId) -> Result<bool, VaultError>;

    /// 检查凭证是否存在
    fn exists(&self, credential_id: &CredentialId) -> Result<bool, VaultError>;

    /// 更新凭证条目
    fn update(&self, entry: &VaultEntry) -> Result<(), VaultError>;

    /// 创建版本记录
    fn create_version_record(&self, version: &CredentialVersion) -> Result<(), VaultError>;

    /// 获取版本历史
    fn get_version_history(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Vec<CredentialVersion>, VaultError>;

    /// 获取指定版本
    fn get_version(
        &self,
        credential_id: &CredentialId,
        version: u32,
    ) -> Result<Option<CredentialVersion>, VaultError>;
}

/// 租户用户索引类型
/// tenant_id -> user_hash -> Vec<credential_id>
type TenantUserIndex = HashMap<String, HashMap<String, Vec<String>>>;

/// 版本历史存储类型
/// credential_id -> version -> CredentialVersion
type VersionHistoryStore = HashMap<String, HashMap<u32, CredentialVersion>>;

/// 内存存储实现（用于测试和开发）
///
/// 生产环境应使用 PostgreSQL + RLS 或 HashiCorp Vault
pub struct InMemoryStorage {
    /// 存储所有凭证条目（按凭证 ID 索引）
    entries: Arc<RwLock<HashMap<String, VaultEntry>>>,

    /// 按租户和用户索引（用于快速查询）
    index: Arc<RwLock<TenantUserIndex>>,

    /// 版本历史存储
    versions: Arc<RwLock<VersionHistoryStore>>,
}

impl InMemoryStorage {
    /// 创建新的内存存储
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            index: Arc::new(RwLock::new(HashMap::new())),
            versions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 构建索引键
    fn build_index_key(tenant_id: &TenantId, user_id: &UserId) -> (String, String) {
        (tenant_id.as_str().to_string(), user_id.hash().to_string())
    }

    /// 更新索引
    fn add_to_index(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        credential_id: &CredentialId,
    ) -> Result<(), VaultError> {
        let mut index = self
            .index
            .write()
            .map_err(|_| VaultError::StorageError("Index lock poisoned".to_string()))?;

        let (t_key, u_key) = Self::build_index_key(tenant_id, user_id);

        index
            .entry(t_key)
            .or_insert_with(HashMap::new)
            .entry(u_key)
            .or_insert_with(Vec::new)
            .push(credential_id.as_str().to_string());

        Ok(())
    }

    /// 从索引移除
    fn remove_from_index(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        credential_id: &CredentialId,
    ) -> Result<(), VaultError> {
        let mut index = self
            .index
            .write()
            .map_err(|_| VaultError::StorageError("Index lock poisoned".to_string()))?;

        let (t_key, u_key) = Self::build_index_key(tenant_id, user_id);

        if let Some(tenant_index) = index.get_mut(&t_key)
            && let Some(user_entries) = tenant_index.get_mut(&u_key)
        {
            user_entries.retain(|id| id != credential_id.as_str());
        }

        Ok(())
    }

    /// 获取用户的所有凭证 ID
    fn get_user_credential_ids(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
    ) -> Result<Vec<String>, VaultError> {
        let index = self
            .index
            .read()
            .map_err(|_| VaultError::StorageError("Index lock poisoned".to_string()))?;

        let (t_key, u_key) = Self::build_index_key(tenant_id, user_id);

        Ok(index
            .get(&t_key)
            .and_then(|t| t.get(&u_key))
            .cloned()
            .unwrap_or_default())
    }

    /// 获取租户的所有条目数量
    pub fn tenant_entry_count(&self, tenant_id: &TenantId) -> Result<usize, VaultError> {
        let index = self
            .index
            .read()
            .map_err(|_| VaultError::StorageError("Index lock poisoned".to_string()))?;

        Ok(index
            .get(tenant_id.as_str())
            .map(|t| t.values().map(|v| v.len()).sum())
            .unwrap_or(0))
    }

    /// 清空所有数据（仅用于测试）
    pub fn clear(&self) -> Result<(), VaultError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;
        let mut index = self
            .index
            .write()
            .map_err(|_| VaultError::StorageError("Index lock poisoned".to_string()))?;

        entries.clear();
        index.clear();

        Ok(())
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StorageBackend for InMemoryStorage {
    fn store(&self, entry: &VaultEntry) -> Result<(), VaultError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;

        // 存储条目
        entries.insert(entry.credential_id.as_str().to_string(), entry.clone());

        // 更新索引
        drop(entries); // 释放锁
        self.add_to_index(&entry.tenant_id, &entry.user_id, &entry.credential_id)?;

        Ok(())
    }

    fn get(&self, credential_id: &CredentialId) -> Result<Option<VaultEntry>, VaultError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;

        Ok(entries.get(credential_id.as_str()).cloned())
    }

    fn get_metadata(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Option<CredentialMetadata>, VaultError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;

        Ok(entries
            .get(credential_id.as_str())
            .map(|entry| entry.metadata()))
    }

    fn query(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        filter: &CredentialFilter,
    ) -> Result<CredentialQueryResult, VaultError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;

        let credential_ids = self.get_user_credential_ids(tenant_id, user_id)?;
        let mut result = Vec::new();

        for id in credential_ids {
            if let Some(entry) = entries.get(&id) {
                // 应用过滤器

                // 跳过已删除（除非包含已删除）
                if entry.is_deleted && !filter.include_deleted {
                    continue;
                }

                // 跳过已过期（如果 only_valid）
                if filter.only_valid && entry.is_expired() {
                    continue;
                }

                // 按服务 ID 过滤
                if let Some(ref service_id) = filter.service_id
                    && entry.service_id != *service_id
                {
                    continue;
                }

                // 按凭证类型过滤
                if let Some(ref cred_type) = filter.credential_type
                    && entry.credential_type != *cred_type
                {
                    continue;
                }

                result.push(entry.metadata());
            }
        }

        let total = result.len();

        Ok(CredentialQueryResult {
            credentials: result,
            total,
        })
    }

    fn delete(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;

        if let Some(entry) = entries.get_mut(credential_id.as_str()) {
            entry.mark_deleted();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn purge(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;

        if let Some(entry) = entries.remove(credential_id.as_str()) {
            // 从索引中移除
            drop(entries);
            self.remove_from_index(&entry.tenant_id, &entry.user_id, credential_id)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn exists(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;

        Ok(entries.contains_key(credential_id.as_str()))
    }

    fn update(&self, entry: &VaultEntry) -> Result<(), VaultError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| VaultError::StorageError("Entries lock poisoned".to_string()))?;

        // 检查条目是否存在
        if !entries.contains_key(entry.credential_id.as_str()) {
            return Err(VaultError::CredentialNotFound(
                entry.credential_id.as_str().to_string(),
            ));
        }

        // 更新条目
        entries.insert(entry.credential_id.as_str().to_string(), entry.clone());

        Ok(())
    }

    fn create_version_record(&self, version: &CredentialVersion) -> Result<(), VaultError> {
        let mut versions = self
            .versions
            .write()
            .map_err(|_| VaultError::StorageError("Versions lock poisoned".to_string()))?;

        let credential_versions = versions
            .entry(version.credential_id.clone())
            .or_insert_with(HashMap::new);

        credential_versions.insert(version.version, version.clone());

        Ok(())
    }

    fn get_version_history(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Vec<CredentialVersion>, VaultError> {
        let versions = self
            .versions
            .read()
            .map_err(|_| VaultError::StorageError("Versions lock poisoned".to_string()))?;

        let result = versions
            .get(credential_id.as_str())
            .map(|credential_versions| {
                let mut version_list: Vec<CredentialVersion> =
                    credential_versions.values().cloned().collect();
                // 按版本号排序
                version_list.sort_by_key(|v| v.version);
                version_list
            })
            .unwrap_or_default();

        Ok(result)
    }

    fn get_version(
        &self,
        credential_id: &CredentialId,
        version: u32,
    ) -> Result<Option<CredentialVersion>, VaultError> {
        let versions = self
            .versions
            .read()
            .map_err(|_| VaultError::StorageError("Versions lock poisoned".to_string()))?;

        Ok(versions
            .get(credential_id.as_str())
            .and_then(|credential_versions| credential_versions.get(&version).cloned()))
    }
}

/// 凭证 Vault（主入口）
///
/// 提供凭证的增删改查接口，强制实施多租户隔离
pub struct CredentialVault {
    /// 存储后端
    backend: Box<dyn StorageBackend>,
}

impl CredentialVault {
    /// 创建新的 Vault（使用内存存储）
    pub fn new_in_memory() -> Self {
        Self {
            backend: Box::new(InMemoryStorage::new()),
        }
    }

    /// 创建新的 Vault（使用自定义存储后端）
    pub fn with_backend(backend: Box<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// 创建凭证
    ///
    /// # Arguments
    /// * `request` - 创建请求
    /// * `encrypted_payload` - 加密后的凭证数据
    ///
    /// # Returns
    /// * `Ok(VaultEntry)` - 创建的凭证条目
    pub fn create_credential(
        &self,
        request: CreateCredentialRequest,
        encrypted_payload: EncryptedPayload,
    ) -> Result<VaultEntry, VaultError> {
        // 验证加密载荷格式
        encrypted_payload.validate()?;

        let entry = VaultEntry::new(
            request.tenant_id,
            request.user_id,
            request.service_id,
            request.credential_type,
            encrypted_payload,
            request.expires_at,
        );

        self.backend.store(&entry)?;

        Ok(entry)
    }

    /// 创建凭证（使用预生成的 credential_id）
    ///
    /// 用于修复加密流程中 credential_id 不一致的问题：
    /// 加密时需要先知道将要使用的 credential_id，以确保密钥派生参数一致
    ///
    /// # Arguments
    /// * `request` - 创建请求
    /// * `encrypted_payload` - 加密后的凭证数据
    /// * `credential_id` - 预生成的凭证 ID
    ///
    /// # Returns
    /// * `Ok(VaultEntry)` - 创建的凭证条目
    pub fn create_credential_with_id(
        &self,
        request: CreateCredentialRequest,
        encrypted_payload: EncryptedPayload,
        credential_id: CredentialId,
    ) -> Result<VaultEntry, VaultError> {
        // 验证加密载荷格式
        encrypted_payload.validate()?;

        let entry = VaultEntry::with_credential_id(
            credential_id,
            request.tenant_id,
            request.user_id,
            request.service_id,
            request.credential_type,
            encrypted_payload,
            request.expires_at,
        );

        self.backend.store(&entry)?;

        Ok(entry)
    }

    /// 根据 ID 获取凭证（仅元数据，不含加密载荷）
    ///
    /// # Arguments
    /// * `credential_id` - 凭证 ID
    /// * `tenant_id` - 租户 ID（用于权限验证）
    /// * `user_id` - 用户 ID（用于权限验证）
    ///
    /// # Returns
    /// * `Ok(Some(CredentialMetadata))` - 凭证元数据
    /// * `Err(VaultError::TenantIsolationViolation)` - 无权限访问
    pub fn get_credential_metadata(
        &self,
        credential_id: &CredentialId,
        tenant_id: &TenantId,
        user_id: &UserId,
    ) -> Result<Option<CredentialMetadata>, VaultError> {
        // 先获取完整条目验证权限
        let entry = self.backend.get(credential_id)?;

        if let Some(entry) = entry {
            // 验证租户隔离
            entry.verify_tenant_access(tenant_id, user_id)?;

            // 返回元数据（不含加密载荷）
            Ok(Some(entry.metadata()))
        } else {
            Ok(None)
        }
    }

    /// 根据 ID 获取凭证（含加密载荷，需要额外权限）
    ///
    /// # Arguments
    /// * `credential_id` - 凭证 ID
    /// * `tenant_id` - 租户 ID（用于权限验证）
    /// * `user_id` - 用户 ID（用于权限验证）
    ///
    /// # Returns
    /// * `Ok(Some(VaultEntry))` - 凭证条目（含加密载荷）
    /// * `Err(VaultError::TenantIsolationViolation)` - 无权限访问
    pub fn get_credential(
        &self,
        credential_id: &CredentialId,
        tenant_id: &TenantId,
        user_id: &UserId,
    ) -> Result<Option<VaultEntry>, VaultError> {
        let entry = self.backend.get(credential_id)?;

        if let Some(entry) = entry {
            // 验证租户隔离
            entry.verify_tenant_access(tenant_id, user_id)?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    /// 查询用户的凭证列表（仅元数据）
    ///
    /// # Arguments
    /// * `tenant_id` - 租户 ID
    /// * `user_id` - 用户 ID
    /// * `filter` - 查询过滤器
    ///
    /// # Returns
    /// * `Ok(CredentialQueryResult)` - 查询结果
    pub fn list_credentials(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        filter: CredentialFilter,
    ) -> Result<CredentialQueryResult, VaultError> {
        self.backend.query(tenant_id, user_id, &filter)
    }

    /// 删除凭证（软删除）
    ///
    /// # Arguments
    /// * `credential_id` - 凭证 ID
    /// * `tenant_id` - 租户 ID（用于权限验证）
    /// * `user_id` - 用户 ID（用于权限验证）
    ///
    /// # Returns
    /// * `Ok(true)` - 删除成功
    /// * `Ok(false)` - 凭证不存在
    /// * `Err(VaultError::TenantIsolationViolation)` - 无权限访问
    pub fn delete_credential(
        &self,
        credential_id: &CredentialId,
        tenant_id: &TenantId,
        user_id: &UserId,
    ) -> Result<bool, VaultError> {
        // 先验证权限
        let entry = self.backend.get(credential_id)?;

        if let Some(entry) = entry {
            entry.verify_tenant_access(tenant_id, user_id)?;
            self.backend.delete(credential_id)
        } else {
            Ok(false)
        }
    }

    /// 物理删除凭证（仅管理员可用）
    ///
    /// # Arguments
    /// * `credential_id` - 凭证 ID
    /// * `tenant_id` - 租户 ID（用于权限验证）
    /// * `user_id` - 用户 ID（用于权限验证）
    ///
    /// # Returns
    /// * `Ok(true)` - 删除成功
    /// * `Ok(false)` - 凭证不存在
    /// * `Err(VaultError::TenantIsolationViolation)` - 无权限访问
    pub fn purge_credential(
        &self,
        credential_id: &CredentialId,
        tenant_id: &TenantId,
        user_id: &UserId,
    ) -> Result<bool, VaultError> {
        // 先验证权限
        let entry = self.backend.get(credential_id)?;

        if let Some(entry) = entry {
            entry.verify_tenant_access(tenant_id, user_id)?;
            self.backend.purge(credential_id)
        } else {
            Ok(false)
        }
    }

    /// 检查凭证是否存在
    pub fn credential_exists(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        self.backend.exists(credential_id)
    }

    /// 更新凭证
    ///
    /// # Arguments
    /// * `credential_id` - 凭证 ID
    /// * `tenant_id` - 租户 ID（用于权限验证）
    /// * `user_id` - 用户 ID（用于权限验证）
    /// * `encrypted_payload` - 新的加密载荷（可选）
    /// * `expires_at` - 新的过期时间（可选）
    ///
    /// # Returns
    /// * `Ok(VaultEntry)` - 更新后的凭证条目
    /// * `Err(VaultError::CredentialNotFound)` - 凭证不存在
    /// * `Err(VaultError::TenantIsolationViolation)` - 无权限访问
    pub fn update_credential(
        &self,
        credential_id: &CredentialId,
        tenant_id: &TenantId,
        user_id: &UserId,
        encrypted_payload: Option<EncryptedPayload>,
        expires_at: Option<Option<u64>>,
    ) -> Result<VaultEntry, VaultError> {
        // 先获取现有条目验证权限
        let mut entry = self
            .backend
            .get(credential_id)?
            .ok_or_else(|| VaultError::CredentialNotFound(credential_id.as_str().to_string()))?;

        // 验证租户隔离
        entry.verify_tenant_access(tenant_id, user_id)?;

        // 更新加密载荷（如果提供）
        if let Some(payload) = encrypted_payload {
            payload.validate()?;
            entry.encrypted_payload = payload;
        }

        // 更新过期时间（如果提供）
        if let Some(exp) = expires_at {
            entry.expires_at = exp;
        }

        // 更新时间戳
        entry.updated_at = current_timestamp();

        // 保存更新
        self.backend.update(&entry)?;

        Ok(entry)
    }

    /// 更新凭证（带版本控制）
    ///
    /// # Arguments
    /// * `credential_id` - 凭证 ID
    /// * `tenant_id` - 租户 ID（用于权限验证）
    /// * `user_id` - 用户 ID（用于权限验证）
    /// * `encrypted_payload` - 新的加密载荷
    /// * `change_reason` - 变更原因
    ///
    /// # Returns
    /// * `Ok(UpdateResult)` - 更新结果
    pub fn update_credential_with_version(
        &self,
        credential_id: &CredentialId,
        tenant_id: &TenantId,
        user_id: &UserId,
        encrypted_payload: EncryptedPayload,
        change_reason: Option<String>,
    ) -> Result<UpdateResult, VaultError> {
        use super::version::CredentialVersion;

        // 先获取现有条目验证权限
        let mut entry = self
            .backend
            .get(credential_id)?
            .ok_or_else(|| VaultError::CredentialNotFound(credential_id.as_str().to_string()))?;

        // 验证租户隔离
        entry.verify_tenant_access(tenant_id, user_id)?;

        let previous_version = entry.version;

        // 将当前版本保存到版本历史
        let version_record = CredentialVersion::new(
            credential_id.as_str().to_string(),
            entry.version,
            entry.encrypted_payload.clone(),
            change_reason.clone(),
            Some(user_id.hash().to_string()),
        );
        self.backend.create_version_record(&version_record)?;

        // 更新凭证内容
        encrypted_payload.validate()?;
        entry.encrypted_payload = encrypted_payload;

        // 增加版本号
        entry.increment_version();

        // 保存更新
        self.backend.update(&entry)?;

        Ok(UpdateResult {
            credential_id: credential_id.as_str().to_string(),
            previous_version,
            new_version: entry.version,
            service_id: entry.service_id.as_str().to_string(),
            credential_type: entry.credential_type.as_str().to_string(),
        })
    }

    /// 获取版本历史
    pub fn get_version_history(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Vec<super::version::CredentialVersion>, VaultError> {
        self.backend.get_version_history(credential_id)
    }

    /// 获取指定版本
    pub fn get_version(
        &self,
        credential_id: &CredentialId,
        version: u32,
    ) -> Result<Option<super::version::CredentialVersion>, VaultError> {
        self.backend.get_version(credential_id, version)
    }

    /// 回滚到指定版本
    pub fn rollback_credential(
        &self,
        credential_id: &CredentialId,
        tenant_id: &TenantId,
        user_id: &UserId,
        target_version: u32,
        reason: &str,
    ) -> Result<RollbackResult, VaultError> {
        use super::version::CredentialVersion;

        // 获取当前凭证验证权限
        let mut entry = self
            .backend
            .get(credential_id)?
            .ok_or_else(|| VaultError::CredentialNotFound(credential_id.as_str().to_string()))?;

        // 验证租户隔离
        entry.verify_tenant_access(tenant_id, user_id)?;

        let current_version = entry.version;

        // 验证目标版本
        if target_version < 1 || target_version >= current_version {
            return Err(VaultError::InvalidVersion {
                message: format!("无效的目标版本: {target_version}, 当前版本: {current_version}"),
            });
        }

        // 获取目标版本记录
        let target_record = self
            .backend
            .get_version(credential_id, target_version)?
            .ok_or_else(|| VaultError::VersionNotFound {
                credential_id: credential_id.as_str().to_string(),
                version: target_version,
            })?;

        // 将当前版本保存到历史
        let current_version_record = CredentialVersion::new(
            credential_id.as_str().to_string(),
            entry.version,
            entry.encrypted_payload.clone(),
            Some(format!("rollback: {reason}")),
            Some(user_id.hash().to_string()),
        );
        self.backend
            .create_version_record(&current_version_record)?;

        // 复制目标版本的加密载荷到当前凭证
        entry.encrypted_payload = target_record.encrypted_payload;

        // 增加版本号（回滚后创建新版本）
        entry.increment_version();

        // 保存更新
        self.backend.update(&entry)?;

        Ok(RollbackResult {
            credential_id: credential_id.as_str().to_string(),
            previous_version: current_version,
            current_version: entry.version,
        })
    }
}

/// 更新结果
pub struct UpdateResult {
    pub credential_id: String,
    pub previous_version: u32,
    pub new_version: u32,
    pub service_id: String,
    pub credential_type: String,
}

/// 回滚结果
pub struct RollbackResult {
    pub credential_id: String,
    pub previous_version: u32,
    pub current_version: u32,
}

/// 获取当前 Unix 时间戳（秒）
fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before Unix epoch")
        .as_secs()
}

/// 创建凭证的便捷函数
///
/// # Example
/// ```rust,ignore
/// let vault = CredentialVault::new_in_memory();
/// let entry = create_credential(
///     &vault,
///     "tenant_123",
///     "user_456",
///     "schwab",
///     CredentialType::UsernamePassword,
///     encrypted_payload,
///     None,
/// )?;
/// ```
pub fn create_credential(
    vault: &CredentialVault,
    tenant_id: impl Into<String>,
    user_id: impl Into<String>,
    service_id: impl Into<String>,
    credential_type: crate::models::CredentialType,
    encrypted_payload: EncryptedPayload,
    expires_at: Option<u64>,
) -> Result<VaultEntry, VaultError> {
    let request = CreateCredentialRequest {
        tenant_id: TenantId::new(tenant_id),
        user_id: UserId::new(user_id),
        service_id: ServiceId::new(service_id),
        credential_type,
        expires_at,
    };

    vault.create_credential(request, encrypted_payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::constants;
    use crate::models::CredentialType;

    fn create_test_payload() -> EncryptedPayload {
        EncryptedPayload::new(
            constants::PROTOCOL_VERSION,
            constants::ALGORITHM_AES_256_GCM,
            constants::KDF_HKDF_SHA256,
            vec![0u8; constants::NONCE_LENGTH],
            vec![0u8; constants::AUTH_TAG_LENGTH],
            vec![1, 2, 3, 4, 5],
        )
    }

    #[test]
    fn test_in_memory_storage_crud() {
        let storage = InMemoryStorage::new();
        let entry = VaultEntry::new(
            TenantId::new("tenant_123"),
            UserId::new("user_456"),
            ServiceId::new("schwab"),
            CredentialType::UsernamePassword,
            create_test_payload(),
            None,
        );

        // 存储
        storage.store(&entry).unwrap();

        // 获取
        let retrieved = storage.get(&entry.credential_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.unwrap().credential_id.as_str(),
            entry.credential_id.as_str()
        );

        // 获取元数据
        let metadata = storage.get_metadata(&entry.credential_id).unwrap();
        assert!(metadata.is_some());
        assert_eq!(metadata.unwrap().service_id, "schwab");

        // 存在检查
        assert!(storage.exists(&entry.credential_id).unwrap());

        // 删除
        let deleted = storage.delete(&entry.credential_id).unwrap();
        assert!(deleted);

        // 确认删除（但条目仍在，只是标记为删除）
        let retrieved = storage.get(&entry.credential_id).unwrap();
        assert!(retrieved.unwrap().is_deleted);

        // 物理删除
        let purged = storage.purge(&entry.credential_id).unwrap();
        assert!(purged);

        // 确认物理删除
        assert!(!storage.exists(&entry.credential_id).unwrap());
    }

    #[test]
    fn test_tenant_isolation_query() {
        let storage = InMemoryStorage::new();

        // 创建不同租户的凭证
        let entry1 = VaultEntry::new(
            TenantId::new("tenant_1"),
            UserId::new("user_a"),
            ServiceId::new("service_1"),
            CredentialType::UsernamePassword,
            create_test_payload(),
            None,
        );

        let entry2 = VaultEntry::new(
            TenantId::new("tenant_1"),
            UserId::new("user_b"),
            ServiceId::new("service_2"),
            CredentialType::ApiKey,
            create_test_payload(),
            None,
        );

        let entry3 = VaultEntry::new(
            TenantId::new("tenant_2"),
            UserId::new("user_c"),
            ServiceId::new("service_3"),
            CredentialType::OAuthRefresh,
            create_test_payload(),
            None,
        );

        storage.store(&entry1).unwrap();
        storage.store(&entry2).unwrap();
        storage.store(&entry3).unwrap();

        // 查询 tenant_1, user_a
        let result = storage
            .query(
                &TenantId::new("tenant_1"),
                &UserId::from_hash(entry1.user_id.hash()),
                &CredentialFilter::default(),
            )
            .unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(result.credentials[0].service_id, "service_1");

        // 查询 tenant_1, user_b
        let result = storage
            .query(
                &TenantId::new("tenant_1"),
                &UserId::from_hash(entry2.user_id.hash()),
                &CredentialFilter::default(),
            )
            .unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(result.credentials[0].service_id, "service_2");

        // 查询 tenant_2
        let result = storage
            .query(
                &TenantId::new("tenant_2"),
                &UserId::from_hash(entry3.user_id.hash()),
                &CredentialFilter::default(),
            )
            .unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(result.credentials[0].service_id, "service_3");
    }

    #[test]
    fn test_credential_filter() {
        let storage = InMemoryStorage::new();

        let tenant_id = TenantId::new("tenant_1");
        let user_id = UserId::new("user_a");

        // 创建不同类型的凭证
        let entry1 = VaultEntry::new(
            tenant_id.clone(),
            user_id.clone(),
            ServiceId::new("schwab"),
            CredentialType::UsernamePassword,
            create_test_payload(),
            None,
        );

        let entry2 = VaultEntry::new(
            tenant_id.clone(),
            user_id.clone(),
            ServiceId::new("github"),
            CredentialType::ApiKey,
            create_test_payload(),
            None,
        );

        let entry3 = VaultEntry::new(
            tenant_id.clone(),
            user_id.clone(),
            ServiceId::new("google"),
            CredentialType::OAuthRefresh,
            create_test_payload(),
            None,
        );

        storage.store(&entry1).unwrap();
        storage.store(&entry2).unwrap();
        storage.store(&entry3).unwrap();

        // 按服务 ID 过滤
        let filter = CredentialFilter {
            service_id: Some(ServiceId::new("schwab")),
            ..Default::default()
        };
        let result = storage
            .query(&tenant_id, &UserId::from_hash(user_id.hash()), &filter)
            .unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.credentials[0].service_id, "schwab");

        // 按凭证类型过滤
        let filter = CredentialFilter {
            credential_type: Some(CredentialType::ApiKey),
            ..Default::default()
        };
        let result = storage
            .query(&tenant_id, &UserId::from_hash(user_id.hash()), &filter)
            .unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(
            result.credentials[0].credential_type,
            CredentialType::ApiKey
        );
    }

    #[test]
    fn test_vault_create_and_retrieve() {
        let vault = CredentialVault::new_in_memory();

        // 创建凭证
        let request = CreateCredentialRequest {
            tenant_id: TenantId::new("tenant_123"),
            user_id: UserId::new("user_456"),
            service_id: ServiceId::new("schwab"),
            credential_type: CredentialType::UsernamePassword,
            expires_at: None,
        };

        let entry = vault
            .create_credential(request, create_test_payload())
            .unwrap();

        // 验证创建成功
        assert!(!entry.credential_id.as_str().is_empty());
        assert_eq!(entry.service_id.as_str(), "schwab");

        // 获取元数据
        let metadata = vault
            .get_credential_metadata(
                &entry.credential_id,
                &TenantId::new("tenant_123"),
                &UserId::from_hash(entry.user_id.hash()),
            )
            .unwrap();

        assert!(metadata.is_some());
        assert_eq!(metadata.unwrap().service_id, "schwab");

        // 获取完整凭证
        let full = vault
            .get_credential(
                &entry.credential_id,
                &TenantId::new("tenant_123"),
                &UserId::from_hash(entry.user_id.hash()),
            )
            .unwrap();

        assert!(full.is_some());
    }

    #[test]
    fn test_vault_tenant_isolation() {
        let vault = CredentialVault::new_in_memory();

        // 创建凭证
        let entry = create_credential(
            &vault,
            "tenant_123",
            "user_456",
            "schwab",
            CredentialType::UsernamePassword,
            create_test_payload(),
            None,
        )
        .unwrap();

        // 正确租户访问
        let metadata = vault
            .get_credential_metadata(
                &entry.credential_id,
                &TenantId::new("tenant_123"),
                &UserId::from_hash(entry.user_id.hash()),
            )
            .unwrap();
        assert!(metadata.is_some());

        // 错误租户访问应该失败
        let result = vault.get_credential_metadata(
            &entry.credential_id,
            &TenantId::new("wrong_tenant"),
            &UserId::from_hash(entry.user_id.hash()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_vault_list_credentials() {
        let vault = CredentialVault::new_in_memory();
        let tenant_id = TenantId::new("tenant_123");
        let user_id = UserId::new("user_456");

        // 创建多个凭证
        create_credential(
            &vault,
            "tenant_123",
            "user_456",
            "schwab",
            CredentialType::UsernamePassword,
            create_test_payload(),
            None,
        )
        .unwrap();

        create_credential(
            &vault,
            "tenant_123",
            "user_456",
            "github",
            CredentialType::ApiKey,
            create_test_payload(),
            None,
        )
        .unwrap();

        // 列出凭证
        let result = vault
            .list_credentials(
                &tenant_id,
                &UserId::from_hash(user_id.hash()),
                CredentialFilter::default(),
            )
            .unwrap();

        assert_eq!(result.total, 2);
    }

    #[test]
    fn test_vault_delete_and_purge() {
        let vault = CredentialVault::new_in_memory();
        let tenant_id = TenantId::new("tenant_123");
        let user_id = UserId::new("user_456");

        // 创建凭证
        let entry = create_credential(
            &vault,
            "tenant_123",
            "user_456",
            "schwab",
            CredentialType::UsernamePassword,
            create_test_payload(),
            None,
        )
        .unwrap();

        // 软删除
        let deleted = vault
            .delete_credential(
                &entry.credential_id,
                &tenant_id,
                &UserId::from_hash(user_id.hash()),
            )
            .unwrap();
        assert!(deleted);

        // 获取元数据（仍存在，但标记为删除）
        let metadata = vault
            .get_credential_metadata(
                &entry.credential_id,
                &tenant_id,
                &UserId::from_hash(user_id.hash()),
            )
            .unwrap();
        assert!(metadata.is_some());
        assert!(metadata.unwrap().is_deleted);

        // 物理删除
        let purged = vault
            .purge_credential(
                &entry.credential_id,
                &tenant_id,
                &UserId::from_hash(user_id.hash()),
            )
            .unwrap();
        assert!(purged);

        // 确认不存在
        let exists = vault.credential_exists(&entry.credential_id).unwrap();
        assert!(!exists);
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let storage = Arc::new(InMemoryStorage::new());
        let mut handles = vec![];

        // 并发写入
        for i in 0..10 {
            let storage = Arc::clone(&storage);
            let handle = thread::spawn(move || {
                let entry = VaultEntry::new(
                    TenantId::new("tenant_1"),
                    UserId::new(format!("user_{}", i)),
                    ServiceId::new(format!("service_{}", i)),
                    CredentialType::ApiKey,
                    create_test_payload(),
                    None,
                );
                storage.store(&entry).unwrap();
                entry.credential_id.as_str().to_string()
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        let ids: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // 验证所有条目都存在
        assert_eq!(ids.len(), 10);
        for id in ids {
            assert!(
                storage
                    .exists(&CredentialId::from_string(id).unwrap())
                    .unwrap()
            );
        }
    }
}
