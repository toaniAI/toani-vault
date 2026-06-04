# ZKM-162 阶段性验收文档

## 任务

- 卡片：`ZKM-162`
- 标题：`[Vault][OAuth Broker] 锁定 OAuth Broker v1 边界与外部命名`

## 阶段目标

在编码开始前锁定 OAuth Broker v1 的边界、术语和兼容性约束，避免后续 provider、binding、transaction、runtime 和 audit 子任务因为范围漂移而返工。

## 验收前提

- 技术设计文档与 PRD 已同步到当前版本。
- 本地仓库包含最新拆解文档与任务定义。
- 负责人已能访问 Multica、Lark 文档与本地仓库。

## 本地优先验证方式

- 基于本地文档与代码现状完成边界审查，不要求第三方 provider 凭据。
- 使用本地全文检索和接口契约审查确认当前对外命名与现有实现一致。

## 验收场景

### AC-162-1 首版范围被明确锁定

- 操作：
  - 对照技术设计文档和 PRD，输出 v1 支持的 grant family、binding type、provider 范围和排除项。
  - 明确是否排除 `device authorization flow`。
- 期望结果：
  - 存在单一结论来源，不再出现“PRD 包含但技术设计排除”的未决冲突。
- 关键证据：
  - 本地决策记录截图或文档片段。
  - Lark 文档中已写明的范围结论链接。
  - Multica 卡片评论中记录的最终边界摘要。

### AC-162-2 外部命名兼容策略被锁定

- 操作：
  - 在本地检查现有 `oauth_refresh`、`oauth_token`、`o_auth_refresh` 的代码与前端使用点。
  - 明确后续是继续兼容还是分阶段迁移。
- 期望结果：
  - API、前端和 SDK 不会在后续任务中各自采用不同名称。
- 关键证据：
  - 本地检索结果：`src/api/credentials.rs`、`src/models/mod.rs`、`frontend/src/features/credentials/pages/CredentialsPage.contracts.ts`。
  - 命名兼容策略文档截图。

### AC-162-3 Admin / Use 平面边界被明确

- 操作：
  - 基于当前 `src/api/auth.rs`、`src/api/tokens.rs`、`src/token/claims.rs`，定义 management token 与 runtime token 的责任分离。
- 期望结果：
  - 后续所有子任务都以统一的 token 语义与命令族边界为准。
- 关键证据：
  - 本地设计结论文档。
  - 当前 claims / issued_from / subject_type 代码引用截图。

## 建议执行命令

```bash
rtk rg -n "oauth_refresh|oauth_token|o_auth_refresh|subject_type|issued_from|credential_ids" src frontend sdk-typescript cli
rtk rg -n "device authorization|client credentials|authorization code|PKCE" docs src frontend
```

## 必备证据包

- 边界结论文档链接
- 命名兼容策略链接
- 本地代码检索输出
- Multica 卡片回链记录

## 不通过条件

- 仍然存在未裁决的首版范围冲突。
- 命名兼容策略没有明确到 API / UI / SDK 层。
- Admin / Use 平面边界无法落到现有 token 语义上。
