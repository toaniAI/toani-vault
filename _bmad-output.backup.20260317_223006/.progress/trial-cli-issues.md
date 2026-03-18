# CredBridge CLI 试用问题清单

## 严重问题

### CLI-001: CLI 无子命令系统
- **问题描述**: 当前 CLI 仅执行固定的演示程序，不接受任何命令行参数
- **重现步骤**: 运行 `cargo run -- --help` 或 `cargo run -- any-command`
- **预期结果**: 应显示帮助文档或执行指定命令
- **实际结果**: 忽略所有参数，直接执行演示
- **影响**: 🔴 高 - 无法使用 CLI 进行任何操作
- **修复建议**: 引入 `clap` crate 实现子命令系统

### CLI-002: 无标准 CLI 参数支持
- **问题描述**: 不支持 `--help`、`-h`、`-V`、`--version` 等标准参数
- **重现步骤**: 运行 `cargo run -- --help`
- **预期结果**: 显示帮助信息
- **实际结果**: 执行演示程序
- **影响**: 🔴 高 - 用户体验差
- **修复建议**: 在 clap 中配置标准参数

### CLI-003: CLI 与 API 功能不匹配
- **问题描述**: API 已实现完整功能，但 CLI 无任何对应命令
- **缺失命令**:
  - 凭证管理: create, update, delete, list, get, decrypt
  - Token 管理: generate, revoke, validate
  - 审计日志: query, export, filter, verify
  - 租户管理: create, configure, switch
  - 系统命令: status, health, version
- **影响**: 🔴 高 - CLI 无法访问核心功能
- **修复建议**: 为每个 API 端点创建 CLI 子命令

---

## 一般问题

### CLI-004: 编译警告过多
- **问题描述**: 54 个编译警告
- **类型分布**:
  - 未使用导入: ~30 个
  - 废弃函数调用: 6 个 (base64::encode/decode)
  - 未使用变量: ~15 个
  - 未读字段: 3 个
- **影响**: 🟡 中 - 代码整洁度
- **修复建议**:
  - 运行 `cargo fix --lib -p vault-service`
  - 手动修复剩余问题

### CLI-005: 使用废弃的 base64 API
- **问题描述**: 代码中使用 `base64::encode` 和 `base64::decode`
- **位置**:
  - `src/api/attestation.rs`: 5 处
  - `src/api/audit.rs`: 1 处
- **影响**: 🟡 中 - 未来版本兼容性
- **修复建议**: 迁移到 `base64::Engine::encode/decode`

---

## 功能缺失清单

### 凭证管理
| 功能 | 优先级 | 状态 |
|------|--------|------|
| `credbridge credential create` | P0 | ❌ 未实现 |
| `credbridge credential update <id>` | P0 | ❌ 未实现 |
| `credbridge credential delete <id>` | P0 | ❌ 未实现 |
| `credbridge credential list` | P0 | ❌ 未实现 |
| `credbridge credential get <id>` | P0 | ❌ 未实现 |
| `credbridge credential decrypt <id>` | P1 | ❌ 未实现 |

### Token 管理
| 功能 | 优先级 | 状态 |
|------|--------|------|
| `credbridge token generate` | P0 | ❌ 未实现 |
| `credbridge token revoke <id>` | P0 | ❌ 未实现 |
| `credbridge token validate <token>` | P0 | ❌ 未实现 |
| `credbridge token refresh <token>` | P1 | ❌ 未实现 |

### 审计日志
| 功能 | 优先级 | 状态 |
|------|--------|------|
| `credbridge audit query` | P1 | ❌ 未实现 |
| `credbridge audit export` | P1 | ❌ 未实现 |
| `credbridge audit filter` | P1 | ❌ 未实现 |
| `credbridge audit verify` | P2 | ❌ 未实现 |

### 租户管理
| 功能 | 优先级 | 状态 |
|------|--------|------|
| `credbridge tenant create` | P1 | ❌ 未实现 |
| `credbridge tenant configure` | P1 | ❌ 未实现 |
| `credbridge tenant switch <id>` | P1 | ❌ 未实现 |
| `credbridge tenant list` | P2 | ❌ 未实现 |

### 系统命令
| 功能 | 优先级 | 状态 |
|------|--------|------|
| `credbridge status` | P1 | ❌ 未实现 |
| `credbridge health` | P0 | ❌ 未实现 |
| `credbridge version` | P0 | ❌ 未实现 |
| `credbridge config` | P2 | ❌ 未实现 |

---

## 改进建议

### 1. CLI 框架选型
推荐使用 `clap` v4，支持：
- 派生宏 (derive macros)
- 子命令
- 参数验证
- 自动生成帮助文档
- shell 自动补全

### 2. 输出格式支持
每个命令应支持 `--output` 参数：
- `json`: 机器可读
- `table`: 人类友好的表格
- `text`: 简单文本（默认）

### 3. 配置文件
支持以下配置文件（按优先级）：
1. 命令行参数
2. 环境变量 (`CREDBRIDGE_*`)
3. 当前目录 `.credbridge.toml`
4. 用户目录 `~/.credbridge/config.toml`
5. 系统配置 `/etc/credbridge/config.toml`

### 4. 错误处理
- 统一的错误格式
- 明确的退出码
- 详细的错误信息（调试模式）

---

## 修复优先级

### 立即修复 (P0)
- CLI-001: 引入子命令系统
- CLI-002: 标准参数支持
- CLI-003: 凭证/Token 核心命令

### 短期修复 (P1)
- CLI-004: 清理编译警告
- CLI-005: 迁移 base64 API
- 系统命令: health, version

### 长期优化 (P2)
- 审计日志命令
- 租户管理命令
- shell 自动补全
