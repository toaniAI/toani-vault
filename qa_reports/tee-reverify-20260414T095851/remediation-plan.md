# TEE 环境二次验证后续修复与排查计划

基于本次二次验证（`tee-reverify-20260414T095851`），将未修复问题拆成两类：

1. 可以通过修改仓库代码、文档、测试与构建产物来解决或显著收敛的问题
2. 主要由外部环境、部署参数、宿主机资源、运行时配置或发布流程造成的问题

本文目标：

- 明确每一项属于哪一类
- 给出可直接执行的代码修复计划
- 对不能仅靠代码修复的问题给出排查计划

---

## 一、分类结论

### A. 可通过代码修复或以代码为主收敛的问题

1. `CLI 非 sandbox 命令清理与文档收敛`
2. `Tokens verify 相关契约清理`
3. `Token / Sandbox 产品契约清理（Developer Center、Tokens/Profile/Credentials 页面、公开文档）`
4. `Token persistence contract / credential_ids 白名单口径收敛`
5. `Sandbox ID mapping 语义不清`

### B. 不能仅靠代码修复，主要是外部环境 / 部署 / 运行时问题

1. `Sandbox execute runtime failure`
2. `Baseline /health /ready /health/detail 返回不符合契约`
3. `Redis runtime token/session state 缺失`

说明：

- `Baseline health/readiness` 虽然仓库中已有正确路由定义，但线上表现与源码不一致，更像发布产物或 Ingress/Nginx 未采用当前配置。
- `Redis runtime state` 有两种可能：
  - 线上服务实际未连到你提供的 Redis DB 3
  - 现网只使用 Redis 黑名单，不再写入“active token/session”旧模型
  这两种都不能只靠“改一段业务代码”来判定。

---

## 二、可通过代码修复的问题与修复计划

## 1. CLI 非 sandbox 命令清理与文档收敛

### 当前证据

- 构建后的 CLI help 仍只有 `sandbox` group
- 但仓库内已经存在：
  - `cli/src/commands/auth.ts`
  - `cli/src/commands/config.ts`
  - `cli/src/commands/credentials.ts`
  - `cli/src/commands/tokens.ts`
  - `cli/src/commands/service-accounts.ts`

### 根因判断

产品决策已经明确：CLI 只提供 sandbox 访问请求。

当前漂移来自两个方向：

1. 仓库里仍保留了 `auth`、`config`、`credentials`、`tokens`、`service-accounts` 等命令实现痕迹
2. README、Developer Center、测试和帮助文案未完全收敛到“CLI 仅支持 sandbox”

### 修复目标

让 CLI 的代码、构建产物和文档全部收敛到一个明确边界：

- `toani` 只保留 `sandbox` command group
- 清理所有非 sandbox CLI 入口、帮助、README、示例和测试
- Developer Center 中所有 CLI 指引都必须基于“sandbox-only CLI”

### 代码修改计划

1. 清理 CLI 源码中的非 sandbox 入口痕迹
   - 审查并删除或隔离：
     - `cli/src/commands/auth.ts`
     - `cli/src/commands/config.ts`
     - `cli/src/commands/credentials.ts`
     - `cli/src/commands/tokens.ts`
     - `cli/src/commands/service-accounts.ts`
   - 原则：
     - 如果这些文件已不再需要，直接删除
     - 如果短期内仍需保留内部参考，则至少不能再被 README、help、测试、发布产物引用
2. 保持 `cli/src/index.ts` 为 sandbox-only 入口
   - `--help` 只显示 `sandbox`
   - 修正 `--version`
     - 当前输出仍写死 `0.0.2`
     - 应改为从单一版本源读取，避免版本漂移
3. 清理 CLI 文档
   - `cli/README.md`
   - `cli/SKILL.md`
   - `cli/SKILL-INSTALL.md`
   - `cli/INTERFACE_BASELINE.md`
   - 删除或改写所有非 sandbox 命令说明
4. 清理仓库级引用
   - Developer Center
   - 用户手册
   - 任何提到 CLI 具备 `auth/config/credentials/tokens/service-accounts/audit` 的文档

### 需要补的测试

建议新增：

- `cli/tests/help.test.ts`
  - 断言 `toani --help` 只包含 `sandbox`
- `cli/tests/version.test.ts`
  - 断言 CLI version 与 package version 一致
- 清理或删除所有依赖非 sandbox 命令的测试
- 保留 sandbox smoke test

### 验证标准

- `npm run build`
- `node dist/index.js --help`
- `node dist/index.js --help` 中不再出现非 sandbox group
- README 与 Developer Center 中不再声明非 sandbox CLI 能力

---

## 2. Tokens verify 相关契约清理

### 当前证据

- 线上 `POST /api/v1/tokens/verify` 返回 `405 Allow: GET,HEAD`
- `src/api/tokens.rs` 当前只注册：
  - `/tokens`
  - `/tokens/:token_id`
  - `/tokens/stats`
  - `/tokens/:token_id/revoke`

### 根因判断

产品决策已经明确：`verify` 相关接口暂不开放。

当前问题不是“缺一个后端实现”，而是“代码、文档、页面、测试、验收口径里仍残留已取消的公开契约”。

### 修复目标

清理所有对外或对验收可见的 `verify` 契约痕迹，确保系统对外声明与真实能力一致：

- 后端不再宣称支持该接口
- 前端 Developer Center 不再展示该接口
- README / SDK / CLI / QA 口径不再将其视为公开能力
- 自动化测试不再把 `verify` 缺失视为失败

### 代码与文档清理计划

1. 清理前端文案与入口
   - `frontend/src/features/developer/pages/DeveloperCenter.tsx`
   - `frontend/src/shared/i18n/messages.ts`
   - 删除 `verify` 相关 API endpoint 展示、示例请求、Tester 模板
2. 清理测试基线
   - 删除或改写前端/集成测试中对 `tokens verify` 的公开契约断言
   - 更新 QA claim 与验收脚本，不再把 `/api/v1/tokens/verify` 作为必测公开接口
3. 清理文档
   - README
   - SDK 示例
   - Developer 文档
   - 任何对外 API 列表或 OpenAPI 生成源
4. 清理残留代码注释
   - 若代码注释、接口列表、常量命名仍暗示未来公开 verify，明确标注“未开放”或直接删除
5. 保留内部能力边界
   - 如果系统内部仍有 token 校验逻辑供中间件使用，不对外暴露即可
- 不新增任何新的公开 verify handler

### 需要补的测试

建议新增或调整：

- 前端内容测试断言 Developer Center 不再展示 `tokens/verify`
- API 契约测试断言公开 token routes 不包含 verify
- QA 验收模板移除该项或将其标注为“未开放，不纳入公开契约”

### 验证标准

- `cd frontend && npm run test:unit`
- 搜索仓库公开文档和页面后，不再出现把 `/api/v1/tokens/verify` 当作开放接口的内容
- 新一轮远程验收中，不再把该项计为失败项

---

## 3. Token / Sandbox 产品契约清理（Developer Center、Tokens/Profile/Credentials 页面、公开文档）

### 当前证据

页面当前仍展示：

- `toani --base-url https://your-api.example.com --token <BEARER_TOKEN> sandbox stats`
- 多处 `https://your-api.example.com`
- 仍残留旧的 token onboarding / automation / sandbox-only 之外的泛化示例

而当前真实产品表面已包含：

- Credentials
- Tokens
- Developer Center
- Users

### 根因判断

当前仓库计划与页面内容还没有完全对齐新的 token 收敛目标：

- token 只用于 TEE sandbox 内的受限凭证占位符解析
- Tokens 页面是唯一人工签发入口
- CLI 只能消费手工签发好的 token 做 sandbox 请求
- 不开放 verify
- 不开放 automation token
- 不开放直接明文 decrypt
- 凭证列表/详情只返回元数据
- `credential:read` 的含义收敛为“受限沙箱凭证使用权”

### 修复目标

让前端页面、共享 API 层与公开文档全部对齐新的 token / sandbox 收敛目标：

- Tokens 页面只保留人工签发 token
- Profile 页面不再暴露 automation token
- Credentials 页面不再出现明文解密/查看/复制能力
- Developer Center 不再展示 verify / decrypt / automation / token management 示例
- Developer Center 和 CLI 文档都明确：
  - token 需从 Dashboard 手工签发
  - token 只能用于 sandbox
  - base URL、payload、示例必须符合当前 API
- 前端共享 API 层移除已取消能力，避免旧页面或测试误调用

### 代码修改计划

重点文件：

- `frontend/src/features/tokens/pages/TokensPage.tsx`
- `frontend/src/features/auth/pages/ProfilePage.tsx`
- `frontend/src/features/credentials/pages/CredentialsPage.tsx`
- `frontend/src/features/developer/pages/DeveloperCenter.tsx`
- `frontend/src/shared/i18n/messages.ts`
- `frontend/src/shared/api/services.ts`
- `frontend/src/shared/api/*`

具体改动：

1. 收敛 Tokens 页面
   - 只保留人工签发 token 入口
   - scope 固定为 `credential:read`
   - 必须选择 `credential_ids` 白名单
   - 删除 verify tab / verify flow / 多 scope 选择
2. 收敛 Profile 页面
   - 删除 automation token UI、hooks、类型和文案
   - 清理所有 `/profile/automation-tokens*` 前端调用
3. 收敛 Credentials 页面
   - 删除“查看/解密明文”入口、弹窗、复制明文字段能力
   - 页面只保留元数据管理
4. 清理前端共享 API 层
   - 删除 `decrypt`
   - 删除 `verify token`
   - 删除 `automation token`
   - 确保无页面可继续触发这些调用
5. 更新 Developer Center
   - 明确写清楚“CLI 只支持 sandbox”
   - 删除任何暗示 CLI 支持其他 command family 的表述
   - 删除 decrypt / verify / automation / CLI token management 示例
   - API Tester 只保留当前公开且允许的 token / sandbox 模板
6. 更新 CLI onboarding
   - 改为“先去 Dashboard 手工签发 token，再在 CLI 中执行 sandbox”
   - 不再提 automation token
7. 更新 SDK 示例
   - 全量替换 `https://your-api.example.com` 为可配置占位模式，或默认 `https://dev-credbridge.bitkinetic.com`
   - 若示例用于文档而非生产，说明“请替换为你的部署地址”
   - 校准到当前 token / sandbox 授权模型
8. 校准接口示例
   - token create 示例必须带 `credential_ids`
   - sandbox 示例必须体现“token 只用于受限凭证占位符解析”
   - 若示例仍用旧参数名，如 `service_id`、旧 decrypt 能力、automation token 模型，需要同步修正
9. 清理公开文档
   - `docs/03-API 参考/REST-API.md`
   - CLI README / SDK 文档 / 用户手册 / Developer 文档
   - 删除 verify、automation token、直接 decrypt 的公开描述
   - 统一写明 Dashboard 是唯一 token 获取入口

### 需要补的测试

建议新增或扩展：

- `frontend/src/features/tokens/pages/TokensPage.*.test.*`
- `frontend/src/features/auth/pages/ProfilePage.*.test.*`
- `frontend/src/features/credentials/pages/CredentialsPage.*.test.*`
- `frontend/src/features/developer/pages/DeveloperCenter.content.test.mjs`
- `frontend/src/features/developer/pages/DeveloperCenter.apiTester.test.mjs`

断言至少包括：

- 不再包含 `https://your-api.example.com`
- CLI 文案明确为 sandbox-only，且不再出现其他命令家族声明
- 页面不再展示 verify endpoint
- 页面不再展示 automation token
- 页面不再展示 decrypt / plaintext 能力
- Tokens 页面必须要求 credential 白名单
- Credentials 页面只显示元数据，不显示明文读取能力

### 验证标准

- `cd frontend && npm run test:unit`
- 登录后打开 `/developer`
- 登录后打开 `/tokens`
- 登录后打开 `/profile`
- 登录后打开 `/credentials`
- 人工检查 `API Docs`、`SDK Examples`、`Tester`

---

## 4. Token persistence contract / credential_ids 白名单口径收敛

### 当前证据

- 新生成 token 会写入 `credbridge_vault.api_tokens`
- `scope_tokens` 对应 token 没有新记录
- 新目标已经明确：授权边界落到 `credential_ids` 白名单，而不是 `scope_tokens` 的旧模型
- 代码中 `auth/service.rs` 只明确写 `api_tokens`
- schema / docs / 验收口径里仍保留 `scope_tokens` 和更宽泛的 token 能力叙述

### 根因判断

这是“持久化模型已经演进到 `api_tokens + credential_ids`，但文档、旧表定义、权限语义和验收口径未完全收敛”的问题。

### 修复目标

- 把 `api_tokens` 认定为当前唯一真实 token metadata 存储
- 把 `credential_ids` 认定为当前唯一资源级授权边界
- 把 `credential:read` 明确定义为“受限沙箱凭证使用权”
- 将 `scope_tokens` 退为遗留表或迁移期兼容对象，不再作为验收主口径
- 清理 `credential:decrypt` 相关历史兼容项、默认角色能力和文档暗示

### 代码与文档修复计划

1. 统一后端 token 权限语义
   - `src/api/middleware.rs`
   - `src/token/claims.rs`
   - `src/token/scope.rs`
   - `src/token/permission.rs`
   - 明确 `credential:read` 的产品含义
2. 清理 `credential:decrypt` 相关残留
   - 默认角色权限
   - 枚举 / 注释 / 测试 / SDK 类型
   - 避免对外仍暴露“可直接解密”的能力暗示
3. 收敛持久化口径
   - 在 `api_tokens` 上明确 `credential_ids` 是真实白名单字段
   - 在 `src/services/db/schema.rs`、注释、报表模板中标注 `scope_tokens` 为 legacy
4. 清理公开 API 文档
   - token create 文档必须写明 `credential_ids` 必填
   - 删除任何把 token 能力描述为通用读/解密/校验/审计访问的内容
5. 清理验收与对账口径
   - DB 验证脚本改为对账 `api_tokens.credential_ids`
   - 不再把 `scope_tokens` 缺失视为失败
6. 清理 automation token / service account token 的公开产品叙事
   - 如果这些能力不再属于对外目标，文档与页面要一并清理
   - 若后端暂时保留内部兼容接口，必须从公开契约中移除

### 需要补的测试

- token create 后，断言 `api_tokens.credential_ids` 正确持久化
- token create 拒绝空 `credential_ids`
- token create 拒绝越权 `credential_ids`
- sandbox / middleware 能从 token 中恢复 `allowed_credential_ids`
- `/credentials/:id/decrypt` 不再返回明文
- 不再把 `scope_tokens` 缺失视为自动失败

### 验证标准

- 统一 README / Developer Center / QA 报告模板 / DB 验证脚本
- 新验收以 `api_tokens + credential_ids` 为主口径
- 新验收不再因 `scope_tokens` 空而误报

---

## 5. Sandbox Session 与 runtime sandbox ID mapping 语义不清

### 当前证据

- API 返回 `session_id` + `sandbox_id`
- DB 中 `sandbox_id` 实际映射 `sandbox_sessions.tee_context_id`
- 但 API / 文档未明确解释两者关系

### 根因判断

不是功能故障，而是对象身份语义和运维排障契约不清。

### 修复目标

让 API、日志、文档、审计明确说明：

- `session_id` 是业务会话主键
- `sandbox_id` 是 runtime / TEE context 标识
- 两者如何映射

### 代码修复计划

1. 更新 `src/api/sandbox.rs` 的响应注释和 OpenAPI/文档说明
2. 在返回体中加入更明确字段名，二选一：
   - 保留 `sandbox_id` 并补 `runtime_context_id`
   - 或显式返回 `session_id` + `tee_context_id`
3. 在 audit/logging 中同时打出两者
4. 更新 Developer Center sandbox API 文档示例

### 需要补的测试

- 创建 session 后断言响应体包含业务 ID 和 runtime ID
- DB 对账脚本明确比对字段映射关系

### 验证标准

- API 文档不再出现“只能靠人工猜测 sandbox_id 对应哪个 DB 字段”

---

## 三、不能仅靠代码修复的问题与排查计划

## 6. Sandbox execute runtime failure

### 当前证据

线上错误：

- `clone(flags=CLONE_NEWNS|CLONE_NEWCGROUP|CLONE_NEWUTS|CLONE_NEWIPC|CLONE_NEWUSER|CLONE_NEWPID|CLONE_NEWNET) failed: No space left on device`

### 判断

这不是典型业务代码 bug，更像宿主机或容器运行时约束：

- user namespace / pid namespace / cgroup namespace 资源上限
- `newuidmap` / `newgidmap` 可用性
- `/proc/sys/user/max_*_namespaces` 配额
- 容器安全策略或 Kubernetes PodSecurity 限制
- cgroup / overlay / tmpfs 配额

### 排查计划

1. 在部署环境节点或容器内检查 namespace 配额
   - `/proc/sys/user/max_user_namespaces`
   - `/proc/sys/user/max_pid_namespaces`
   - `/proc/sys/user/max_net_namespaces`
2. 检查 `newuidmap` / `newgidmap` 是否存在且可执行
3. 检查容器是否允许：
   - `CLONE_NEWUSER`
   - `CLONE_NEWNET`
   - `CLONE_NEWNS`
4. 检查是否有 cgroup 或 seccomp 拒绝
   - `dmesg`
   - container runtime logs
   - kubelet / containerd / Docker logs
5. 检查 sandbox 工作目录容量和 inode
   - `/tmp/sandbox/...`
   - 容器 overlay 空间
6. 检查 warm pool 与实际并发
   - 当前 stats 已显示 `Insufficient warm instances: 1/2`
   - 需要确认资源不足是否导致 runtime fallback / clone 失败

### 可做的代码侧辅助，但不是根修复

- 在 sandbox 初始化时增加 preflight self-check
- 将 namespace quota、uidmap availability、tmpfs 可写性作为健康项暴露
- 将 `success: true` 包裹 `data.success: false` 的返回语义改得更清晰

---

## 7. Baseline `/health` `/ready` `/health/detail` 不符合契约

### 当前证据

仓库代码：

- `src/main.rs` 已将 `/ready` 和 `/health/detail` 路由到 detail handler

线上结果：

- `/ready` 返回 SPA HTML
- `/health`、`/health/detail` 返回 plain text `healthy`

### 判断

更像“部署未使用当前仓库服务路由或前端代理配置”，不是单纯后端代码缺失。

仓库已有线索：

- `frontend/nginx.conf` 里对 `/ready`、`/health/detail` 有专门 proxy 配置
- 线上结果表明这个配置没有实际生效，或被更上层 Ingress 覆盖

### 排查计划

1. 确认实际发布的前端 Nginx 配置
   - 是否包含仓库内 `frontend/nginx.conf` 的对应 location
2. 检查 Ingress / API Gateway 路由优先级
   - 是否把 `/ready` 错误地转发到了前端 SPA
3. 对比当前部署镜像版本与仓库 commit
4. 直接绕过前端网关访问后端 service
   - 验证后端原生 `/ready` `/health/detail` 是否正确
5. 若后端原生正确、网关错误：
   - 修网关
6. 若后端原生也错误：
   - 再回头检查 runtime binary 是否旧版本

### 可做的代码侧辅助，但不是根修复

- 增加端到端发布校验脚本，发布后自动验证三条 health 路由
- 在前端构建或 Docker 镜像中加入 Nginx config 快照校验

---

## 8. Redis runtime token/session state 缺失

### 当前证据

- 代码要求 `REDIS_URL` 存在，并初始化 token blacklist store
- 线上 Redis db `3` 只有 `CHAIN_ERROR_WHITE_LIST`
- 本次 token revoke 成功，但 `token:blacklist:*` 和 `credbridge:*` 在该 DB 中都未出现

### 判断

优先怀疑外部配置问题，而不是直接改业务逻辑：

1. 线上服务实际使用的 `REDIS_URL` 不是你提供的 db `3`
2. 线上连接的 Redis 不是这台
3. token blacklist 前缀或 DB 被部署参数覆盖
4. 当前服务只把 Redis 用于黑名单，但写到别的实例/库

### 排查计划

1. 在运行中的服务容器内打印 `REDIS_URL`
2. 核对：
   - host
   - port
   - password
   - db index
3. 在服务容器内直接执行一次 revoke，然后在同一 Redis 连接上查询：
   - `token:blacklist:*`
4. 检查是否存在环境差异：
   - app container 用 DB 0
   - 验收使用 DB 3
5. 若确认现网确实不再依赖 active token/session redis state：
   - 更新文档与验收标准
   - 不再以 `credbridge:tokens:*` 缺失作为失败条件

### 只有在确认产品目标后才做的代码修复

若产品明确要求恢复 `credbridge:tokens:{tenant}:active` 与 `credbridge:token:{jti}`：

1. 在 token issue 流程中补 `store_claims`
2. 在 token revoke 流程中同步更新 Redis active / revoked / metadata
3. 加上远程 Redis 集成测试

但在确认目标前，不建议直接补写，以免引入“双真相源”。

---

## 四、推荐执行顺序

### 第一优先级：代码可控、收益高

1. 清理 `CLI` 非 sandbox 命令与文档
2. 清理 `Tokens verify` 相关契约
3. 收敛 `Token / Sandbox` 产品契约
4. 收敛 `api_tokens + credential_ids` 持久化与权限口径
5. 补 `sandbox id mapping` 说明

### 第二优先级：并行排查外部问题

1. 排查 `sandbox execute runtime failure`
2. 排查 `health/readiness` 实际网关与镜像版本
3. 排查 `Redis` 实际连接目标与 DB index

---

## 五、每项完成后的回归验证

## 代码项回归

- `cargo test`
- `cargo fmt`
- `cargo clippy --tests -- -D warnings`
- `cd frontend && npm run test:unit`
- `cd cli && npm test && npm run build`

## 远程环境回归

1. 登录页面
2. `/developer` 三个 tab 文案复核
3. 确认公开文档、页面、测试中不再暴露 `tokens/verify`
4. 确认 `/tokens` 页面只支持手工签发 `credential:read + credential_ids`
5. 确认 `/profile` 页面不再暴露 automation token
6. 确认 `/credentials` 页面不再暴露 decrypt / plaintext
7. CLI build 后 `toani --help` 仅保留 `sandbox`
8. `GET /health`
9. `GET /ready`
10. `GET /health/detail`
11. `create-session -> execute -> get-operation`
12. Postgres / Redis 对账

---

## 六、建议产出物

建议拆成两个并行工作流：

### 代码修复 PR

- 目标：收敛所有可代码修复项
- 应包含：
  - CLI 非 sandbox 能力清理
  - token verify 契约清理
  - Tokens/Profile/Credentials/Developer Center 契约清理
  - `api_tokens + credential_ids` 持久化、权限语义与测试收敛
  - sandbox ID 文档和测试收敛

### 环境排查工单

- 目标：修复 execute 与 health/readiness / Redis 配置问题
- 应记录：
  - 当前部署镜像 tag / commit
  - 实际 Nginx / Ingress 配置
  - 实际 `REDIS_URL`
  - namespace/cgroup 相关内核参数
  - 容器安全策略与运行时限制
