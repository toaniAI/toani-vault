//! CredBridge SDK - 沙箱模块

use crate::{
    client::CredBridgeClient,
    types::{
        ExecuteSandboxOperationRequest, ExecuteSandboxOperationResponse, RequestOptions, Result,
        SandboxOperationDetail,
    },
};
use std::sync::Arc;

/// 沙箱服务
#[derive(Debug, Clone)]
pub struct SandboxService {
    client: Arc<CredBridgeClient>,
}

impl SandboxService {
    /// 创建新的沙箱服务
    pub fn new(client: Arc<CredBridgeClient>) -> Self {
        Self { client }
    }

    /// 提交无会话 broker 请求
    pub async fn request(
        &self,
        request: ExecuteSandboxOperationRequest,
        options: Option<RequestOptions>,
    ) -> Result<ExecuteSandboxOperationResponse> {
        self.client
            .post_with_options("/sandbox/http-requests", request, options)
            .await
    }

    /// 获取 broker 请求详情
    pub async fn get_request(
        &self,
        operation_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<SandboxOperationDetail> {
        self.client
            .get_with_options(
                &format!("/sandbox/http-requests/{}", operation_id.as_ref()),
                options,
            )
            .await
    }
}
