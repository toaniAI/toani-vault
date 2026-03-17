//! API 路由定义
//!
//! 定义 HTTP API 端点

/// 凭证路由
pub mod credentials {
    /// 创建凭证
    pub const CREATE: &str = "/credentials";
    /// 获取凭证列表
    pub const LIST: &str = "/credentials";
    /// 获取单个凭证
    pub const GET: &str = "/credentials/:id";
    /// 更新凭证
    pub const UPDATE: &str = "/credentials/:id";
    /// 删除凭证
    pub const DELETE: &str = "/credentials/:id";
    /// 解密凭证
    pub const DECRYPT: &str = "/credentials/:id/decrypt";
    /// 获取凭证版本历史
    pub const VERSIONS: &str = "/credentials/:id/versions";
    /// 获取指定版本详情
    pub const VERSION_DETAIL: &str = "/credentials/:id/versions/:version";
    /// 回滚凭证到指定版本
    pub const ROLLBACK: &str = "/credentials/:id/rollback";
}

/// Token 路由
pub mod tokens {
    /// 创建 Token
    pub const CREATE: &str = "/tokens";
    /// 验证 Token
    pub const VERIFY: &str = "/tokens/verify";
    /// 撤销 Token
    pub const REVOKE: &str = "/tokens/:id/revoke";
}

/// 审计日志路由
pub mod audit {
    /// 查询审计日志列表
    pub const LIST: &str = "/audit/logs";
    /// 获取审计日志详情
    pub const GET: &str = "/audit/logs/:id";
    /// 导出审计日志
    pub const EXPORT: &str = "/audit/export";
    /// 验证审计日志
    pub const VERIFY: &str = "/audit/verify";
}

/// 健康检查路由
pub mod health {
    /// 健康检查
    pub const CHECK: &str = "/health";
    /// 详细健康检查
    pub const CHECK_DETAIL: &str = "/health/detail";
    /// 指标端点
    pub const METRICS: &str = "/metrics";
}

/// 沙箱路由
pub mod sandbox {
    /// 创建沙箱会话
    pub const CREATE_SESSION: &str = "/sandbox/sessions";
    /// 获取沙箱会话列表
    pub const LIST_SESSIONS: &str = "/sandbox/sessions";
    /// 获取单个沙箱会话
    pub const GET_SESSION: &str = "/sandbox/sessions/:id";
    /// 终止沙箱会话
    pub const TERMINATE_SESSION: &str = "/sandbox/sessions/:id/terminate";
    /// 在沙箱中执行操作
    pub const EXECUTE_OPERATION: &str = "/sandbox/sessions/:id/execute";
    /// 获取会话操作列表
    pub const LIST_OPERATIONS: &str = "/sandbox/sessions/:id/operations";
    /// 获取单个操作详情
    pub const GET_OPERATION: &str = "/sandbox/operations/:operation_id";
    /// 获取沙箱统计信息
    pub const GET_STATS: &str = "/sandbox/stats";
    /// 获取沙箱健康状态
    pub const HEALTH_CHECK: &str = "/sandbox/health";
    /// 验证 TEE 证明报告
    pub const VERIFY_ATTESTATION: &str = "/sandbox/attestation/verify";
}
