//! CredBridge SDK - 审计日志模块

use crate::{
    client::CredBridgeClient,
    types::{
        AuditExportRequest, AuditExportResponse, AuditLogFilter, AuditLogsResponse,
        AuditVerifyRequest, AuditVerifyResponse, RequestOptions, Result,
    },
};
use std::sync::Arc;

/// 审计日志服务
#[derive(Debug, Clone)]
pub struct AuditService {
    client: Arc<CredBridgeClient>,
}

impl AuditService {
    /// 创建新的审计日志服务
    pub fn new(client: Arc<CredBridgeClient>) -> Self {
        Self { client }
    }

    /// 查询审计日志
    pub async fn logs(
        &self,
        filter: AuditLogFilter,
        options: Option<RequestOptions>,
    ) -> Result<AuditLogsResponse> {
        let mut query = Vec::new();
        if let Some(v) = filter.start_time {
            query.push(format!("start_time={v}"));
        }
        if let Some(v) = filter.end_time {
            query.push(format!("end_time={v}"));
        }
        if let Some(v) = filter.action {
            query.push(format!("action={}", urlencoding::encode(&v)));
        }
        if let Some(v) = filter.user_id_hash {
            query.push(format!("user_id_hash={}", urlencoding::encode(&v)));
        }
        if let Some(v) = filter.limit {
            query.push(format!("page_size={v}"));
        }
        if let Some(v) = filter.offset {
            let page_size = filter.limit.unwrap_or(20).max(1);
            query.push(format!("page={}", (v / page_size) + 1));
        }
        let path = if query.is_empty() {
            "/audit/logs".to_string()
        } else {
            format!("/audit/logs?{}", query.join("&"))
        };
        self.client.get_with_options(&path, options).await
    }

    /// 导出审计日志
    pub async fn export(
        &self,
        request: AuditExportRequest,
        options: Option<RequestOptions>,
    ) -> Result<AuditExportResponse> {
        self.client
            .post_with_options("/audit/export", request, options)
            .await
    }

    /// 校验审计日志
    pub async fn verify(
        &self,
        request: AuditVerifyRequest,
        options: Option<RequestOptions>,
    ) -> Result<AuditVerifyResponse> {
        self.client
            .post_with_options("/audit/verify", request, options)
            .await
    }
}
