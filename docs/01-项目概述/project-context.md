---
project_name: 'credbridge'
user_name: 'CoPaw'
date: '2026-03-18'
sections_completed:
  - 'technology_stack'
  - 'language_rules'
  - 'framework_rules'
  - 'testing_rules'
  - 'code_quality_rules'
  - 'workflow_rules'
  - 'critical_rules'
status: 'complete'
rule_count: 75
optimized_for_llm: true
---

# Project Context for AI Agents

_This file contains critical rules and patterns that AI agents must follow when implementing code in this project. Focus on unobvious details that agents might otherwise miss._

---

## Technology Stack & Versions

### Backend (Rust)
- **Edition**: 2024
- **Framework**: Axum 0.7 with Tower 0.4
- **Database**: SQLx 0.8 (PostgreSQL, `runtime-tokio`, `tls-native-tls`)
- **Async Runtime**: Tokio 1.x (`full`, `rt-multi-thread`)
- **Cache**: Redis 0.24 (`tokio-comp`, `connection-manager`)
- **Vault**: vaultrs 0.7
- **Token**: pasetors 0.7 (PASETO v4 only - NOT JWT)
- **Crypto**: ring 0.17, aes-gcm 0.10, ed25519-dalek 2.1
- **Security**: zeroize 1.8 (memory clearing), constant_time_eq 0.3
- **Error Handling**: thiserror 1.0

### Frontend (React/TypeScript)
- **Framework**: React 19.2.0
- **Language**: TypeScript 5.9.3 (Strict mode enabled)
- **Build**: Vite 7.3.1
- **Styling**: Tailwind CSS 3.4.1, tw-animate-css 1.4.0
- **UI Components**: shadcn/ui 4.0.5, Radix UI primitives
- **State Management**: Zustand 5.0.11
- **Data Fetching**: TanStack Query 5.90.21
- **Routing**: React Router 7.13.1
- **Icons**: Lucide React 0.577.0

### Infrastructure & Security
- **Database**: PostgreSQL with Row Level Security (RLS)
- **Secrets**: HashiCorp Vault
- **TEE**: Intel SGX/TDX with DCAP remote attestation
- **Token Format**: PASETO v4 (local mode) - NEVER use JWT

---

## Critical Implementation Rules

### Language-Specific Rules

#### TypeScript/React
- **Strict Mode**: `strict: true`, `noUnusedLocals: true`, `noUnusedParameters: true` - 代码必须通过这些检查
- **Path Aliases**: 始终使用 `@/` 导入项目模块，禁止相对路径如 `../../../`
- **Export Pattern**: 组件使用具名导出 `export function Component()`，不是默认导出
- **No `any`**: 禁止 `any` 类型，使用 `unknown` 或具体类型
- **Discriminated Unions**: 异步状态使用联合类型：`{ status: 'idle' | 'loading' | 'success' | 'error' }`
- **Hook Dependencies**: `useEffect`, `useCallback` 必须包含完整依赖数组
- **Cleanup**: `useEffect` 返回清理函数处理订阅

#### Rust
- **Module Order**: 1) Imports 2) Constants 3) Types 4) impl 5) Traits 6) Tests
- **No unwrap/expect**: 生产代码禁止使用，使用 `?` 操作符
- **Error Type**: 使用 `thiserror` 定义自定义错误类型，不要直接用 `anyhow`
- **SQLx Errors**: 映射到自定义错误，例如 `RowNotFound` → `ServiceError::NotFound`
- **Security**:
  - 敏感数据使用 `#[derive(Zeroize, ZeroizeOnDrop)]`
  - 密钥比较使用 `constant_time_eq` (不是 `==`)
  - 所有输入必须验证
  - SQLx 使用参数化查询 (`.bind()`)
- **Async**: 使用 `tokio::time::timeout` 包装外部调用
- **Documentation**: 所有公共 API 必须文档注释 `///`

### Framework-Specific Rules

#### React/Frontend
- **Component Structure**:
  1. Imports 2) Types 3) Component function 4) State 5) Hooks 6) Handlers 7) Render helpers 8) Return
- **Props Naming**: 接口命名为 `[ComponentName]Props`
- **Styling**: 使用 `cn()` 工具函数 (来自 `@/lib/utils`) 合并 Tailwind 类
- **State Management**:
  - 本地: `useState` / `useReducer`
  - 共享: Zustand (stores 在 `src/stores/`)
  - 服务器: TanStack Query (`useQuery`, `useMutation`)
  - URL: React Router
- **Data Fetching**:
  - 始终使用 TanStack Query，禁止裸 `fetch` 或 `axios` 直接调用
  - Query keys: `['credentials']`, `['credential', id]`, `['audit', 'logs']`
  - Mutations 成功后 invalidate: `queryClient.invalidateQueries({ queryKey: ['credentials'] })`
  - 默认配置: `staleTime: 5 * 60 * 1000` (5分钟)
- **Performance**:
  - 传递给子组件的回调使用 `useCallback`
  - 列表超过 100 项使用虚拟化 (`react-window`)

#### Axum/Backend
- **Route Organization**: 路由在 `src/api/routes.rs`，中间件在 `src/api/middleware.rs`
- **Response Format**: 使用 `src/api/response.rs` 中的统一响应包装器
- **Error Handling**: 错误转换为 HTTP 响应使用标准格式
- **Middleware Order**: CORS → Auth → Rate Limit → Tenant → Route Handler
- **Database**: 使用 `sqlx::query_as` 配合强类型，禁止裸 SQL 字符串拼接

### Testing Rules

#### Rust Tests
- **Unit Tests**: 同一文件 `#[cfg(test)]` 模块，使用 `#[tokio::test]` 异步测试
- **Integration Tests**: `tests/` 目录，每功能一个文件 (如 `tests/token/paseto_tests.rs`)
- **Database Tests**: 使用 `sqlx::test` 宏，需要 `DATABASE_URL` 环境变量
- **Mocking**: 使用 `mockall`，预期调用使用 `predicate::*`
- **Test Naming**: `test_[function]_[scenario]`，例如 `test_authenticate_valid_token`
- **Required Test Features**:
  - `paseto_tests`, `redis_store_tests`, `scope_tests`
  - `audit_events_tests`, `immudb_tests`, `tenant_middleware_tests`
  - `dcap_tests`, `attestation_api_tests`, `rls_integration`
  - `sandbox_export_tests`, `sandbox_performance`

#### Frontend Tests
- **E2E**: Playwright (已配置在 `package.json`)
- **Test Location**: `frontend/tests/` 目录
- **Security Tests**: 负面测试用例必须覆盖错误路径

#### Coverage Requirements
- Rust 单元测试: >70%
- 关键路径: 必须有集成测试
- 安全代码: 必须有负面测试 (攻击场景)

### Code Quality & Style Rules

#### Linting & Formatting (Frontend)
- **ESLint**: 9.x with `@eslint/js` + `typescript-eslint` + `eslint-plugin-react-hooks`
- **Prettier**: 3.8.1
- **Scripts**: `npm run lint`, `npm run lint:fix`, `npm run format`, `npm run format:check`
- **Pre-commit**: 必须通过这些检查

#### File Organization
**Frontend (`frontend/src/`)**:
- `components/` - UI 组件 (shadcn/ui + 自定义)
- `pages/` - 页面级组件
- `hooks/` - 自定义 React Hooks
- `stores/` - Zustand stores
- `lib/` - 工具函数 (含 `utils.ts` 的 `cn()`)

**Backend (`src/`)**:
- `api/` - Axum 路由和中间件
- `crypto/` - 加密功能 (AES-GCM, PASETO, keys)
- `models/` - 数据模型和数据库实体
- `services/` - 业务逻辑服务
- `tee/` - 可信执行环境 (DCAP, sandbox)
- `vault/` - HashiCorp Vault 集成
- `token/` - PASETO Token 处理
- `audit/` - 审计日志

#### Naming Conventions
**Rust**:
- Modules: `snake_case`
- Types (struct, enum, trait): `PascalCase`
- Functions: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Generic params: `T`, `U` (单字母) 或描述性名称

**TypeScript**:
- Components: `PascalCase` (如 `UserCard`)
- Hooks: `camelCase` 以 `use` 开头 (如 `useAuth`)
- Props 接口: `[Component]Props`
- Types: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`
- Files: 与默认导出组件同名

#### Documentation Requirements
- **Rust**: 所有 `pub` 项必须 `///` 文档注释，包含 Examples
- **TypeScript**: 复杂类型和公共函数需要 JSDoc

### Development Workflow Rules

#### Agent Team Routing (CLAUDE.md)
执行任务前，根据类型路由到正确的 Agent：

| 任务类型 | Agent | 触发条件 |
|---------|-------|---------|
| UI组件、页面、样式、交互 | frontend-specialist | React/TS/Tailwind/shadcn 修改 |
| API、数据库、业务逻辑、安全 | backend-specialist | Rust/Axum/SQLx/加密/认证 |
| 需求分析、功能规划、用户故事 | product-manager | 新功能规划、需求澄清 |
| 测试策略、测试用例、E2E、质量 | qa-engineer | 测试计划、测试实现 |
| 代码审查、架构评审、安全审查 | code-reviewer | 所有代码变更必须通过 |

#### Workflow Chains
- **新功能**: 规划 → product-manager(需求) → specialist(实现) → code-reviewer
- **Bug修复**: 分类 → specialist(修复) → code-reviewer
- **重构**: 规划 → code-reviewer(审查计划) → specialist(执行) → code-reviewer

#### MetaMemory Shared Knowledge
- **开始工作前**: `mm search <query>` 查找现有上下文
- **完成后**: `mm create -t "Title" -c "Content"` 保存决策
- **更新时**: `mm update <doc-id> -c "New content"`
- **追踪**: 创建/更新时使用 `--by "agent-name"`

#### Planning Rules
- 非平凡任务 (3+ 步骤或架构决策) 必须进入规划模式
- 如果出现问题，**立即停止**并重新规划 — 不要硬撑
- 提前编写详细规范以减少歧义

#### Verification Rules
- **禁止**标记任务完成而不证明其工作
- 运行测试、检查日志、证明正确性
- 问自己："资深工程师会认可这个吗？"

### Critical Don't-Miss Rules

#### Security Anti-Patterns (NEVER DO)
- **NEVER use JWT** - Always use PASETO v4 local tokens (`pasetors` crate)
- **NEVER log secrets** - Keys, passwords, tokens must never appear in logs
- **NEVER use `==` for secrets** - Use `constant_time_eq` for timing attack protection
- **NEVER unwrap/expect in production** - Use `?` operator and proper error handling
- **NEVER raw SQL strings** - Always use parameterized queries with `.bind()`
- **NEVER trust user input** - Validate all inputs before processing
- **NEVER skip RLS checks** - Database queries must respect tenant isolation

#### Code Anti-Patterns (NEVER DO)
- **NEVER bare fetch/axios** - Always use TanStack Query for data fetching
- **NEVER relative imports** - Use `@/` path aliases only
- **NEVER `any` type** - Use specific types or `unknown` with type guards
- **NEVER missing cleanup** - `useEffect` must return cleanup functions
- **NEVER incomplete deps** - Hook dependency arrays must be complete

#### TEE/Sandbox Specific Rules
- TEE environment has restricted system calls
- Sandbox code runs under `nsjail` isolation
- DCAP attestation must verify certificate chain
- Sealed data is bound to TEE identity

#### Edge Cases to Handle
- **Token Expiration**: Sync Redis TTL with PASETO expiration
- **Multi-tenancy**: Every DB query must filter by `tenant_id`
- **Key Rotation**: Support dual keys during rotation period
- **Audit Logging**: All credential access must be logged immutably
- **Rate Limiting**: Check before expensive operations
- **Circuit Breaker**: Handle Vault/Redis connection failures gracefully

#### Project-Specific Gotchas
- **PASETO only** - This is a hard requirement, not a suggestion
- **RLS required** - PostgreSQL Row Level Security must be enabled
- **Zeroize** - All sensitive data must derive `Zeroize` trait
- **SQLx offline** - CI builds use `SQLX_OFFLINE=true` with `.sqlx/` queries
- **Feature flags** - Use `strict-production` for release builds (disables debug)

---

---

## Usage Guidelines

**For AI Agents:**

- Read this file before implementing any code
- Follow ALL rules exactly as documented
- When in doubt, prefer the more restrictive option
- Update this file if new patterns emerge

**For Humans:**

- Keep this file lean and focused on agent needs
- Update when technology stack changes
- Review quarterly for outdated rules
- Remove rules that become obvious over time

_Last Updated: 2026-03-18_
