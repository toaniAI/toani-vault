use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "toani",
    about = "Toani Vault CLI - credential management command line tool",
    version,
    author,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format (table, json)
    #[arg(short, long, global = true, default_value = "table")]
    pub output: String,

    /// Config file path
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// Verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Authentication management (login, status)
    #[command(subcommand)]
    Auth(AuthCommands),

    /// Credential management (create, read, update, delete)
    #[command(subcommand)]
    Credentials(CredentialCommands),

    /// Token operations
    #[command(subcommand)]
    Tokens(TokenCommands),

    /// Sandbox session control
    #[command(subcommand)]
    Sandbox(SandboxCommands),

    /// Audit logs
    #[command(subcommand)]
    Audit(AuditCommands),

    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Subcommand)]
pub enum AuthCommands {
    /// 登录到 Toani Vault 服务（仅限服务账户和平台 API Token）
    ///
    /// 此认证方法仅用于服务账户和平台 API Token。
    /// 用户认证请通过 Web 界面使用 Privy 钱包登录。
    Login {
        /// 服务 URL
        #[arg(short, long, env = "CREDBRIDGE_URL")]
        url: String,

        /// API Token（平台 API Token，非用户认证）
        #[arg(short, long, env = "CREDBRIDGE_TOKEN")]
        token: String,

        /// 确认此 Token 为服务账户 Token
        #[arg(long)]
        service_account: bool,
    },
    /// 查看当前登录状态
    Status,
    /// 注销
    Logout,
}

#[derive(Subcommand)]
pub enum CredentialCommands {
    /// 列出所有凭证
    List {
        /// 按类型过滤
        #[arg(short, long)]
        credential_type: Option<String>,

        /// 分页限制
        #[arg(short, long, default_value = "20")]
        limit: u32,

        /// 分页偏移
        #[arg(long, default_value = "0")]
        offset: u32,
    },
    /// 获取单个凭证
    Get {
        /// 凭证 ID
        id: String,

        /// 包含加密值
        #[arg(short, long)]
        include_value: bool,
    },
    /// 创建新凭证
    Create {
        /// 凭证名称
        #[arg(short, long)]
        name: String,

        /// 凭证类型
        #[arg(short, long)]
        credential_type: String,

        /// 凭证值 (可从 stdin 读取)
        #[arg(short, long)]
        value: Option<String>,

        /// 描述
        #[arg(long)]
        description: Option<String>,

        /// 元数据 (key=value 格式)
        #[arg(short, long, value_parser = parse_key_val)]
        metadata: Vec<(String, String)>,
    },
    /// 更新凭证
    Update {
        /// 凭证 ID
        id: String,

        /// 新名称
        #[arg(short, long)]
        name: Option<String>,

        /// 新值
        #[arg(short, long)]
        value: Option<String>,

        /// 新描述
        #[arg(long)]
        description: Option<String>,
    },
    /// 删除凭证
    Delete {
        /// 凭证 ID
        id: String,

        /// 强制删除，不确认
        #[arg(short, long)]
        force: bool,
    },
    /// 解密凭证值
    Decrypt {
        /// 凭证 ID
        id: String,

        /// 版本号
        #[arg(short, long)]
        version: Option<i32>,
    },
    /// 查看版本历史
    Versions {
        /// 凭证 ID
        id: String,
    },
    /// 回滚到指定版本
    Rollback {
        /// 凭证 ID
        id: String,
        /// 版本号
        version: i32,
    },
}

#[derive(Subcommand)]
pub enum TokenCommands {
    /// 创建新 Token
    Create {
        /// Token 名称
        #[arg(short, long)]
        name: String,

        /// 过期时间 (秒)
        #[arg(short, long, default_value = "3600")]
        expires_in: u64,

        /// 权限范围 (逗号分隔)
        #[arg(short, long)]
        scopes: Option<String>,
    },
    /// 列出所有 Token
    List,
    /// 撤销 Token
    Revoke {
        /// Token ID
        id: String,
    },
    /// 验证 Token 有效性
    Verify {
        /// Token 值 (可选，默认使用当前)
        token: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum SandboxCommands {
    /// 创建沙箱会话
    CreateSession {
        /// 凭证 ID
        #[arg(long)]
        credential_id: String,

        /// 原始意图描述
        #[arg(long)]
        original_intent: String,
    },
    /// 列出沙箱会话
    ListSessions,
    /// 获取会话详情
    GetSession {
        /// 会话 ID
        id: String,
    },
    /// 终止会话
    Terminate {
        /// 会话 ID
        id: String,

        /// 强制终止
        #[arg(short, long)]
        force: bool,
    },
    /// 执行操作
    Execute {
        /// 会话 ID
        session_id: String,

        /// 操作类型
        #[arg(short, long)]
        operation_type: String,

        /// 操作参数 (JSON)
        #[arg(short, long)]
        params: Option<String>,
    },
    /// 获取操作结果
    GetOperation {
        /// 操作 ID
        operation_id: String,
    },
    /// 查看沙箱统计
    Stats,
}

#[derive(Subcommand)]
pub enum AuditCommands {
    /// 查询审计日志
    Logs {
        /// 开始时间 (ISO8601)
        #[arg(short, long)]
        from: Option<String>,

        /// 结束时间 (ISO8601)
        #[arg(short, long)]
        to: Option<String>,

        /// 操作类型过滤
        #[arg(short, long)]
        action: Option<String>,

        /// 资源类型过滤
        #[arg(long)]
        resource_type: Option<String>,

        /// 限制条数
        #[arg(short, long, default_value = "50")]
        limit: u32,
    },
    /// 导出审计日志
    Export {
        /// 输出文件路径
        output: String,

        /// 格式 (json, csv)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// 开始时间
        #[arg(short, long)]
        from: Option<String>,

        /// 结束时间
        #[arg(short, long)]
        to: Option<String>,
    },
    /// 验证审计日志完整性
    Verify,
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// 初始化配置
    Init {
        /// 服务 URL
        #[arg(short, long)]
        url: Option<String>,

        /// API Token
        #[arg(short, long)]
        token: Option<String>,
    },
    /// 查看配置
    Show,
    /// 设置配置项
    Set {
        /// 键
        key: String,
        /// 值
        value: String,
    },
    /// 获取配置项
    Get {
        /// 键
        key: String,
    },
}

// 辅助函数: 解析 key=value
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}
