use crate::tee::sandbox::{error::SandboxError, types::SessionId};
use async_trait::async_trait;
use redis::{AsyncCommands, Client as RedisClient};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

const OWNER_KEY_PREFIX: &str = "credbridge:sandbox:owner";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxOwnerRecord {
    pub owner_id: String,
    pub base_url: String,
    pub session_id: Uuid,
    pub tenant_id: Uuid,
    pub expires_at: i64,
}

#[async_trait]
pub trait SandboxOwnerRegistry: Send + Sync {
    async fn register_owner(
        &self,
        record: SandboxOwnerRecord,
        ttl: Duration,
    ) -> Result<(), SandboxError>;

    async fn get_owner(
        &self,
        session_id: SessionId,
    ) -> Result<Option<SandboxOwnerRecord>, SandboxError>;

    async fn delete_owner(&self, session_id: SessionId) -> Result<(), SandboxError>;

    async fn delete_owners_for_owner_id(&self, owner_id: &str) -> Result<usize, SandboxError>;
}

#[derive(Clone)]
pub struct RedisSandboxOwnerRegistry {
    client: RedisClient,
}

impl RedisSandboxOwnerRegistry {
    pub fn new(redis_url: &str) -> Result<Self, SandboxError> {
        let client = RedisClient::open(redis_url).map_err(|error| {
            SandboxError::Config(format!(
                "failed to initialize sandbox owner registry: {error}"
            ))
        })?;
        Ok(Self { client })
    }
}

#[async_trait]
impl SandboxOwnerRegistry for RedisSandboxOwnerRegistry {
    async fn register_owner(
        &self,
        record: SandboxOwnerRecord,
        ttl: Duration,
    ) -> Result<(), SandboxError> {
        let ttl_secs = ttl.as_secs().max(1);
        let payload = serde_json::to_string(&record).map_err(|error| {
            SandboxError::Serialization(format!(
                "failed to serialize sandbox owner record for session {}: {error}",
                record.session_id
            ))
        })?;
        let mut connection = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| {
                SandboxError::Other(format!(
                    "failed to connect to sandbox owner registry: {error}"
                ))
            })?;
        connection
            .set_ex(
                owner_key(SessionId::from(record.session_id)),
                payload,
                ttl_secs,
            )
            .await
            .map_err(|error| {
                SandboxError::Other(format!(
                    "failed to persist sandbox owner mapping for session {}: {error}",
                    record.session_id
                ))
            })
    }

    async fn get_owner(
        &self,
        session_id: SessionId,
    ) -> Result<Option<SandboxOwnerRecord>, SandboxError> {
        let mut connection = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| {
                SandboxError::Other(format!(
                    "failed to connect to sandbox owner registry: {error}"
                ))
            })?;
        let payload: Option<String> =
            connection
                .get(owner_key(session_id))
                .await
                .map_err(|error| {
                    SandboxError::Other(format!(
                        "failed to read sandbox owner mapping for session {}: {error}",
                        Uuid::from(session_id)
                    ))
                })?;
        payload
            .map(|raw| {
                serde_json::from_str::<SandboxOwnerRecord>(&raw).map_err(|error| {
                    SandboxError::Serialization(format!(
                        "failed to deserialize sandbox owner mapping for session {}: {error}",
                        Uuid::from(session_id)
                    ))
                })
            })
            .transpose()
    }

    async fn delete_owner(&self, session_id: SessionId) -> Result<(), SandboxError> {
        let mut connection = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| {
                SandboxError::Other(format!(
                    "failed to connect to sandbox owner registry: {error}"
                ))
            })?;
        let _: usize = connection
            .del(owner_key(session_id))
            .await
            .map_err(|error| {
                SandboxError::Other(format!(
                    "failed to delete sandbox owner mapping for session {}: {error}",
                    Uuid::from(session_id)
                ))
            })?;
        Ok(())
    }

    async fn delete_owners_for_owner_id(&self, owner_id: &str) -> Result<usize, SandboxError> {
        let mut connection = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| {
                SandboxError::Other(format!(
                    "failed to connect to sandbox owner registry: {error}"
                ))
            })?;
        let keys: Vec<String> = connection
            .keys(format!("{OWNER_KEY_PREFIX}:*"))
            .await
            .map_err(|error| {
                SandboxError::Other(format!(
                    "failed to enumerate sandbox owner mappings for owner {owner_id}: {error}"
                ))
            })?;

        let mut keys_to_delete = Vec::new();
        for key in keys {
            let payload: Option<String> = connection.get(&key).await.map_err(|error| {
                SandboxError::Other(format!(
                    "failed to inspect sandbox owner mapping {key} for owner {owner_id}: {error}"
                ))
            })?;
            let Some(payload) = payload else {
                continue;
            };
            let record = serde_json::from_str::<SandboxOwnerRecord>(&payload).map_err(|error| {
                SandboxError::Serialization(format!(
                    "failed to deserialize sandbox owner mapping {key} for owner {owner_id}: {error}"
                ))
            })?;
            if record.owner_id == owner_id {
                keys_to_delete.push(key);
            }
        }

        if keys_to_delete.is_empty() {
            return Ok(0);
        }

        let deleted: usize = connection.del(keys_to_delete).await.map_err(|error| {
            SandboxError::Other(format!(
                "failed to delete sandbox owner mappings for owner {owner_id}: {error}"
            ))
        })?;
        Ok(deleted)
    }
}

fn owner_key(session_id: SessionId) -> String {
    format!("{OWNER_KEY_PREFIX}:{}", Uuid::from(session_id))
}
