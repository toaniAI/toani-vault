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
