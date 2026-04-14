# TEE 环境修复清单

基于本轮真实环境验收结果整理，目标是按优先级推进环境恢复、契约对齐和文档修正。

总体验收结论见：

- [summary.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/summary.md)
- [claim-matrix.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/claim-matrix.md)
- [gate-decision.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/gate-decision.md)

## P0

### 1. 修复 Sandbox Execute 在 TEE 环境中的运行时失败

现象：

- `sandbox` 会话创建、查询、终止都能成功。
- `execute` 在真实环境失败，导致系统不能完成最关键的受控执行能力。

影响：

- 当前环境不能被视为“运行时完整可用”。
- 这是本轮 gate fail 的核心阻塞项。

优先处理：

1. 复核 `nsjail` 策略编译/加载失败原因。
2. 对照容器/部署配置检查运行时依赖、挂载路径、策略文件生成逻辑。
3. 复跑 `create-session -> execute -> get-operation`，确认 Loki 日志中的错误已消失。

问题记录与证据：

- [api-sandbox/result.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-sandbox/result.json)
- [api-sandbox/notes.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-sandbox/notes.md)
- [api-sandbox/raw/execute.response.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-sandbox/raw/execute.response.json)
- [api-sandbox/raw/get-operation.response.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-sandbox/raw/get-operation.response.json)
- [api-sandbox/raw/logmcp-nsjail-query.txt](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-sandbox/raw/logmcp-nsjail-query.txt)
- [db/raw/sandbox_sessions.tsv](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/raw/sandbox_sessions.tsv)
- [db/raw/sandbox_operations.tsv](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/raw/sandbox_operations.tsv)

### 2. 修复 CLI 实装与 README 的严重漂移

现象：

- 当前 shipped CLI 只暴露 `sandbox` command group。
- README 中声明的 `config`、`auth`、`credentials`、`tokens`、`audit`、`service-accounts` 在二进制里不存在。

影响：

- 当前 CLI 无法按公开文档接入。
- 用户面对的是“文档可用、程序不可用”的状态。

优先处理：

1. 明确产品目标：恢复 README 声明的命令面，或收缩 README 到真实能力。
2. 若保留现有 README，需优先补齐 `config init` 与基本 `auth/credentials/tokens/audit` 命令。
3. 改善 CLI 失败时的错误输出，避免空 stdout/stderr。

问题记录与证据：

- [cli/result.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/cli/result.json)
- [cli/notes.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/cli/notes.md)
- [cli/raw](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/cli/raw)
- [cli/README.md](/Users/yvan/AIWorkspace/credbridge/cli/README.md)

### 3. 修复 Baseline 健康检查与公开契约不一致

现象：

- `/ready` 返回的是 SPA shell。
- `/health` 与 `/health/detail` 返回纯文本 `healthy`，而不是文档/源码预期的 JSON 语义。

影响：

- 监控、自动化探测、验收基线都会被误导。
- 环境健康面不具备稳定的机器可判定性。

优先处理：

1. 明确这些路径应由前端网关还是后端服务响应。
2. 确保 `/health`、`/ready`、`/health/detail` 返回稳定 JSON 契约。
3. 修复反向代理/Ingress 路由覆盖问题，避免落到前端 SPA。

问题记录与证据：

- [baseline/result.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/baseline/result.json)
- [baseline/notes.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/baseline/notes.md)
- [baseline/raw](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/baseline/raw)

## P1

### 4. 修复 Tokens Verify 契约与实际接口不一致

现象：

- `create/get/list/stats/revoke` 可用。
- 文档声明的 `POST /api/v1/tokens/verify` 不按公开校验接口工作，返回 `401`，同时 `Allow` 暗示该路径并非当前预期方法。

影响：

- Token 验证接口无法按对外契约使用。
- SDK、CLI、文档、集成方都可能实现错误调用。

优先处理：

1. 明确 verify 的真实接口设计，是 `POST` 还是其他形式。
2. 统一后端实现、OpenAPI/文档、前端说明。
3. 为 verify 加回归测试，避免再次漂移。

问题记录与证据：

- [api-tokens/result.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-tokens/result.json)
- [api-tokens/notes.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-tokens/notes.md)
- [api-tokens/raw](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/api-tokens/raw)

### 5. 修复 Developer Center 文案过时

现象：

- 页面仍在指导用户走旧的 token/bootstrap 路径。
- CLI 范围被描述成 sandbox-only。
- SDK/示例里仍写死 `localhost` 风格地址。

影响：

- 即使后端可用，开发者入口仍会把用户引向错误接入方式。
- 这是典型的“契约层产品缺陷”。

优先处理：

1. 更新 CLI bootstrap 指引到当前推荐路径。
2. 删除或修正过时的 sandbox token 交换说明。
3. 把示例 base URL 与当前部署/README 对齐。

问题记录与证据：

- [web-developer-center/result.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-developer-center/result.json)
- [web-developer-center/notes.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-developer-center/notes.md)
- [web-developer-center/developer-api-visible.txt](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-developer-center/developer-api-visible.txt)
- [web-developer-center/developer-sdk-visible.txt](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-developer-center/developer-sdk-visible.txt)
- [cli/README.md](/Users/yvan/AIWorkspace/credbridge/cli/README.md)

### 6. 明确 Token 持久化真实模型并修正文档/对账口径

现象：

- 本轮 DB 对账确认 token 实际落在 `credbridge_vault.api_tokens`。
- `scope_tokens` 对本轮记录是空的。

影响：

- 如果系统目标已经变更，文档与运维认知需要同步。
- 如果目标未变更，则属于持久化模型偏差。

优先处理：

1. 决定 `api_tokens` 是否是当前唯一真实表。
2. 若是，统一文档、验证器、审计口径。
3. 若不是，补齐写入或迁移流程。

问题记录与证据：

- [db/result.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/result.json)
- [db/notes.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/notes.md)
- [db/raw/api_tokens.tsv](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/raw/api_tokens.tsv)
- [db/raw/scope_tokens.tsv](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/raw/scope_tokens.tsv)

## P2

### 7. 补齐或解释 Redis Token Runtime State

现象：

- Redis DB `3` 中没有任何预期的 credbridge token/session runtime keys。
- 仅发现无关键 `CHAIN_ERROR_WHITE_LIST`。

影响：

- 如果 Redis 本应承载 token/session runtime state，则当前环境不完整。
- 如果系统已不再依赖 Redis 承载这部分状态，测试口径和文档应同步调整。

优先处理：

1. 明确 Redis 在当前架构里是否仍承担 token/session runtime state。
2. 若承担，修复 key 写入和 TTL 行为。
3. 若不承担，更新文档、监控和验收规则，避免再按旧假设报错。

问题记录与证据：

- [redis/result.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/redis/result.json)
- [redis/notes.md](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/redis/notes.md)
- [redis/raw/redis_probe.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/redis/raw/redis_probe.json)
- [redis/raw/keys-all.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/redis/raw/keys-all.json)
- [redis/raw/key-CHAIN_ERROR_WHITE_LIST.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/redis/raw/key-CHAIN_ERROR_WHITE_LIST.json)

### 8. 明确 Sandbox Session 与 Runtime Sandbox ID 的映射语义

现象：

- 运行时 `sandbox_id` 映射到 `sandbox_sessions.tee_context_id`，不是 `sandbox_sessions.id`。

影响：

- 对账、日志定位、运维排障容易混淆对象身份。

优先处理：

1. 在 API、日志、DB 文档中统一说明这两个 ID 的角色。
2. 如果可能，在响应或审计里同时输出两者映射关系。

问题记录与证据：

- [db/result.json](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/result.json)
- [db/raw/sandbox_sessions.tsv](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/raw/sandbox_sessions.tsv)
- [db/raw/sandbox_context_lookup.tsv](/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/db/raw/sandbox_context_lookup.tsv)

## 建议修复顺序

1. 先修 `Sandbox execute`。
2. 然后修 `CLI surface` 与 `baseline health/readiness`。
3. 再修 `token verify`、`Developer Center`、`token persistence contract`。
4. 最后确认 `Redis runtime state` 和 `sandbox id mapping` 是否属于真实缺陷还是文档/验收口径问题。
