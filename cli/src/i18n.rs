pub fn current_locale() -> &'static str {
    let env_locale = std::env::var("CREDBRIDGE_LOCALE")
        .ok()
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_else(|| "en-US".to_string());

    let normalized = env_locale.to_ascii_lowercase();
    if normalized.starts_with("zh") {
        "zh-CN"
    } else {
        "en-US"
    }
}

pub fn tr(key: &str) -> &str {
    match (current_locale(), key) {
        ("zh-CN", "cli.not_configured") => "未配置，请先运行 'credbridge auth login'",
        ("en-US", "cli.not_configured") => {
            "CLI is not configured. Run 'credbridge auth login' first."
        }
        ("zh-CN", "cli.auth.connecting") => "🔌 正在连接到",
        ("en-US", "cli.auth.connecting") => "🔌 Connecting to",
        ("zh-CN", "cli.auth.login_success") => "✅ 登录成功!",
        ("en-US", "cli.auth.login_success") => "✅ Login successful!",
        ("zh-CN", "cli.auth.connection_failed") => "连接失败",
        ("en-US", "cli.auth.connection_failed") => "Connection failed",
        ("zh-CN", "cli.auth.config_saved") => "配置已保存。",
        ("en-US", "cli.auth.config_saved") => "Configuration saved.",
        ("zh-CN", "cli.auth.not_logged_in") => "⚠️  未登录",
        ("en-US", "cli.auth.not_logged_in") => "⚠️  Not logged in.",
        ("zh-CN", "cli.auth.run_login") => "请运行: credbridge auth login",
        ("en-US", "cli.auth.run_login") => "Run: credbridge auth login",
        ("zh-CN", "cli.auth.checking_status") => "🔍 检查登录状态...",
        ("en-US", "cli.auth.checking_status") => "🔍 Checking login status...",
        ("zh-CN", "cli.auth.logged_in") => "已登录",
        ("en-US", "cli.auth.logged_in") => "Logged in",
        ("zh-CN", "cli.auth.token_valid") => "有效",
        ("en-US", "cli.auth.token_valid") => "Valid",
        ("zh-CN", "cli.auth.login_invalid") => "登录无效",
        ("en-US", "cli.auth.login_invalid") => "Login invalid",
        ("zh-CN", "cli.auth.error") => "错误",
        ("en-US", "cli.auth.error") => "Error",
        ("zh-CN", "cli.auth.relogin") => "请重新登录: credbridge auth login",
        ("en-US", "cli.auth.relogin") => "Please log in again: credbridge auth login",
        ("zh-CN", "cli.auth.logged_out") => "✅ 已登出，配置已删除",
        ("en-US", "cli.auth.logged_out") => "✅ Logged out and removed saved configuration.",
        ("zh-CN", "cli.auth.service") => "服务",
        ("en-US", "cli.auth.service") => "Service",
        ("zh-CN", "cli.credentials.listing") => "🔍 正在获取凭证列表...",
        ("en-US", "cli.credentials.listing") => "🔍 Fetching credentials...",
        ("zh-CN", "cli.credentials.empty") => "ℹ️  暂无凭证",
        ("en-US", "cli.credentials.empty") => "ℹ️  No credentials found.",
        ("zh-CN", "cli.credentials.total") => "records",
        ("en-US", "cli.credentials.total") => "records",
        ("zh-CN", "cli.credentials.fetch_failed") => "获取凭证失败",
        ("en-US", "cli.credentials.fetch_failed") => "Failed to fetch credential",
        ("zh-CN", "cli.credentials.details") => "凭证详情:",
        ("en-US", "cli.credentials.details") => "Credential details:",
        ("zh-CN", "cli.credentials.prompt_value") => "请输入凭证值",
        ("en-US", "cli.credentials.prompt_value") => "Enter credential value",
        ("zh-CN", "cli.credentials.creating") => "📝 正在创建凭证...",
        ("en-US", "cli.credentials.creating") => "📝 Creating credential...",
        ("zh-CN", "cli.credentials.created") => "Credential created",
        ("en-US", "cli.credentials.created") => "Credential created",
        ("zh-CN", "cli.credentials.update_unavailable") => "更新凭证功能暂不可用",
        ("en-US", "cli.credentials.update_unavailable") => "Credential update is not available yet",
        ("zh-CN", "cli.credentials.use_delete_create") => "请使用 delete + create 来更新凭证",
        ("en-US", "cli.credentials.use_delete_create") => {
            "Use delete + create to replace a credential."
        }
        ("zh-CN", "cli.credentials.delete_confirm") => "确定要删除凭证",
        ("en-US", "cli.credentials.delete_confirm") => "Delete credential",
        ("zh-CN", "cli.credentials.cancelled") => "已取消",
        ("en-US", "cli.credentials.cancelled") => "Cancelled.",
        ("zh-CN", "cli.credentials.deleting") => "🗑️  正在删除凭证...",
        ("en-US", "cli.credentials.deleting") => "🗑️  Deleting credential...",
        ("zh-CN", "cli.credentials.deleted") => "已删除",
        ("en-US", "cli.credentials.deleted") => "deleted",
        ("zh-CN", "cli.credentials.decrypting") => "🔓 正在解密凭证...",
        ("en-US", "cli.credentials.decrypting") => "🔓 Decrypting credential...",
        ("zh-CN", "cli.credentials.data") => "凭证数据:",
        ("en-US", "cli.credentials.data") => "Credential data:",
        ("zh-CN", "cli.credentials.versions_unimplemented") => "ℹ️  版本历史功能暂未实现",
        ("en-US", "cli.credentials.versions_unimplemented") => {
            "ℹ️  Version history is not implemented yet."
        }
        ("zh-CN", "cli.credentials.rollback_unimplemented") => "ℹ️  回滚功能暂未实现",
        ("en-US", "cli.credentials.rollback_unimplemented") => {
            "ℹ️  Rollback is not implemented yet."
        }
        ("zh-CN", "cli.tokens.create_unimplemented") => "ℹ️  Token 创建功能暂未实现",
        ("en-US", "cli.tokens.create_unimplemented") => {
            "ℹ️  Token creation is not implemented yet."
        }
        ("zh-CN", "cli.tokens.use_web") => "请通过 CredBridge Web 界面创建 Token",
        ("en-US", "cli.tokens.use_web") => "Use the CredBridge web interface to create tokens.",
        ("zh-CN", "cli.tokens.list_unimplemented") => "ℹ️  Token 列表功能暂未实现",
        ("en-US", "cli.tokens.list_unimplemented") => "ℹ️  Token listing is not implemented yet.",
        ("zh-CN", "cli.tokens.revoking") => "🚫 正在撤销当前 Token...",
        ("en-US", "cli.tokens.revoking") => "🚫 Revoking current token...",
        ("zh-CN", "cli.tokens.revoked") => "✅ Token 已撤销",
        ("en-US", "cli.tokens.revoked") => "✅ Token revoked.",
        ("zh-CN", "cli.tokens.revoke_failed") => "撤销 Token 失败",
        ("en-US", "cli.tokens.revoke_failed") => "Failed to revoke token",
        ("zh-CN", "cli.tokens.verify_failed") => "验证失败",
        ("en-US", "cli.tokens.verify_failed") => "Verification failed",
        ("zh-CN", "cli.tokens.current_token") => "当前配置的 Token",
        ("en-US", "cli.tokens.current_token") => "current configured token",
        ("zh-CN", "cli.tokens.verifying") => "🔍 正在验证 Token",
        ("en-US", "cli.tokens.verifying") => "🔍 Verifying token",
        ("zh-CN", "cli.tokens.valid") => "Token 有效",
        ("en-US", "cli.tokens.valid") => "Token is valid",
        ("zh-CN", "cli.tokens.invalid") => "Token 无效或已被撤销",
        ("en-US", "cli.tokens.invalid") => "Token is invalid or revoked",
        ("zh-CN", "cli.audit.logs_unimplemented") => "📋 审计日志查询功能暂未实现",
        ("en-US", "cli.audit.logs_unimplemented") => {
            "📋 Audit log querying is not implemented yet."
        }
        ("zh-CN", "cli.audit.web_hint") => "请通过 CredBridge Web 界面查看审计日志",
        ("en-US", "cli.audit.web_hint") => "Use the CredBridge web interface to view audit logs.",
        ("zh-CN", "cli.audit.export_unimplemented") => "📤 审计日志导出功能暂未实现",
        ("en-US", "cli.audit.export_unimplemented") => {
            "📤 Audit log export is not implemented yet."
        }
        ("zh-CN", "cli.audit.verify_unimplemented") => "🔐 审计日志完整性验证功能暂未实现",
        ("en-US", "cli.audit.verify_unimplemented") => {
            "🔐 Audit log verification is not implemented yet."
        }
        ("zh-CN", "cli.sandbox.sdk_failed") => "创建 SDK 客户端失败",
        ("en-US", "cli.sandbox.sdk_failed") => "Failed to create SDK client",
        ("zh-CN", "cli.sandbox.create") => "🏖️  正在创建沙箱会话...",
        ("en-US", "cli.sandbox.create") => "🏖️  Creating sandbox session...",
        ("zh-CN", "cli.sandbox.list") => "🔍 正在获取沙箱会话列表...",
        ("en-US", "cli.sandbox.list") => "🔍 Fetching sandbox sessions...",
        ("zh-CN", "cli.sandbox.get") => "🔍 正在获取会话详情...",
        ("en-US", "cli.sandbox.get") => "🔍 Fetching session details...",
        ("zh-CN", "cli.sandbox.terminate") => "🛑 正在终止会话...",
        ("en-US", "cli.sandbox.terminate") => "🛑 Terminating session...",
        ("zh-CN", "cli.sandbox.execute") => "⚡ 正在执行操作...",
        ("en-US", "cli.sandbox.execute") => "⚡ Executing operation...",
        ("zh-CN", "cli.sandbox.operation_result") => "🔍 正在获取操作结果...",
        ("en-US", "cli.sandbox.operation_result") => "🔍 Fetching operation result...",
        ("zh-CN", "cli.sandbox.stats") => "📊 正在获取沙箱统计...",
        ("en-US", "cli.sandbox.stats") => "📊 Fetching sandbox stats...",
        ("zh-CN", "cli.sandbox.unimplemented") => "ℹ️  沙箱功能暂未实现",
        ("en-US", "cli.sandbox.unimplemented") => "ℹ️  Sandbox support is not implemented yet.",
        ("zh-CN", "cli.config.enter_url") => "请输入 CredBridge 服务 URL",
        ("en-US", "cli.config.enter_url") => "Enter CredBridge service URL",
        ("zh-CN", "cli.config.enter_token") => "请输入 API Token",
        ("en-US", "cli.config.enter_token") => "Enter API token",
        ("zh-CN", "cli.config.saved_to") => "✅ 配置已保存到",
        ("en-US", "cli.config.saved_to") => "✅ Configuration saved to",
        ("zh-CN", "cli.config.invalid_format") => "无效的格式，可选: json, table",
        ("en-US", "cli.config.invalid_format") => "Invalid format. Supported values: json, table",
        ("zh-CN", "cli.config.timeout_number") => "timeout 必须是数字",
        ("en-US", "cli.config.timeout_number") => "timeout must be a number",
        ("zh-CN", "cli.config.unknown_key") => "未知配置项",
        ("en-US", "cli.config.unknown_key") => "Unknown config key",
        ("zh-CN", "cli.config.updated") => "✅ 配置已更新",
        ("en-US", "cli.config.updated") => "✅ Configuration updated",
        _ => key,
    }
}
