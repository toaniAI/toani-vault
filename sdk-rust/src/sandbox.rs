//! CredBridge SDK - 沙箱模块

use crate::{
    client::CredBridgeClient,
    types::{
        ApiSuccess, CreateSandboxSessionRequest, CreateSandboxSessionResponse,
        ExecuteSandboxOperationRequest, ExecuteSandboxOperationResponse, RequestOptions, Result,
        SandboxOperationDetail, SandboxSessionActionResponse, SandboxSessionDetail,
        SandboxSessionsResponse, SandboxStatsResponse,
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

    /// 创建会话
    pub async fn create_session(
        &self,
        request: CreateSandboxSessionRequest,
        options: Option<RequestOptions>,
    ) -> Result<ApiSuccess<CreateSandboxSessionResponse>> {
        self.client
            .post_with_options("/sandbox/sessions", request, options)
            .await
    }

    /// 列出会话
    pub async fn list_sessions(
        &self,
        options: Option<RequestOptions>,
    ) -> Result<ApiSuccess<SandboxSessionsResponse>> {
        self.client
            .get_with_options("/sandbox/sessions", options)
            .await
    }

    /// 获取会话详情
    pub async fn get_session(
        &self,
        session_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<ApiSuccess<SandboxSessionDetail>> {
        self.client
            .get_with_options(&format!("/sandbox/sessions/{}", session_id.as_ref()), options)
            .await
    }

    /// 关闭会话
    pub async fn terminate_session(
        &self,
        session_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<ApiSuccess<SandboxSessionActionResponse>> {
        self.client
            .delete_with_options(&format!("/sandbox/sessions/{}", session_id.as_ref()), options)
            .await
    }

    /// 执行操作
    pub async fn execute(
        &self,
        session_id: impl AsRef<str>,
        request: ExecuteSandboxOperationRequest,
        options: Option<RequestOptions>,
    ) -> Result<ApiSuccess<ExecuteSandboxOperationResponse>> {
        self.client
            .post_with_options(
                &format!("/sandbox/sessions/{}/execute", session_id.as_ref()),
                request,
                options,
            )
            .await
    }

    /// 读取统计
    pub async fn stats(
        &self,
        options: Option<RequestOptions>,
    ) -> Result<ApiSuccess<SandboxStatsResponse>> {
        self.client
            .get_with_options("/sandbox/stats", options)
            .await
    }

    /// 获取操作详情
    pub async fn get_operation(
        &self,
        operation_id: impl AsRef<str>,
        options: Option<RequestOptions>,
    ) -> Result<ApiSuccess<SandboxOperationDetail>> {
        self.client
            .get_with_options(
                &format!("/sandbox/operations/{}", operation_id.as_ref()),
                options,
            )
            .await
    }
}
