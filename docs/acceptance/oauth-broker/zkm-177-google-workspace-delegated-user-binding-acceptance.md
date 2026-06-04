# ZKM-177 阶段性验收文档

## 任务

- 卡片：`ZKM-177`
- 标题：`[Vault][OAuth Broker] 交付 Google Workspace Delegated User Binding 闭环`

## 阶段目标

在第二个 delegated provider 上复用 broker 核心模块，验证 transaction、callback、binding、runtime 和 subject metadata 模型具有平台级复用性。

## 验收前提

- `ZKM-171`、`ZKM-172`、`ZKM-173` 已完成。
- 已准备测试用 Google Workspace OAuth 配置和最小调用目标。
- 本地或开发回调方案可用。

## 本地优先验证方式

- 本地采集 transaction、callback、binding 和 runtime 证据。
- 只把最终用户授权和 Google API 调用作为外部依赖。

## 验收场景

### AC-177-1 Google Workspace delegated auth transaction 可启动

- 操作：
  - 本地发起 start 流程并检查 transaction 状态。
- 期望结果：
  - transaction 可追踪，scope 与 subject 目标明确。
- 关键证据：
  - start 响应。
  - transaction 持久化记录。

### AC-177-2 callback、exchange 与 binding 创建成功

- 操作：
  - 完成真实或受控授权。
  - 检查 callback 校验、code exchange、binding 生成。
- 期望结果：
  - 核心安全门先执行，再进入 provider-specific 适配。
- 关键证据：
  - 浏览器授权截图。
  - callback 处理日志。
  - binding 数据记录。

### AC-177-3 subject metadata、scope snapshot 与 runtime 调用可验证

- 操作：
  - 用生成的 binding 调用一次目标 Google Workspace API。
- 期望结果：
  - subject metadata、scope snapshot 和运行时结果均可审计与解释。
- 关键证据：
  - 本地响应。
  - 审计事件。
  - 远程调用记录。

## 建议执行命令

```bash
TEE_MODE=simulation RUST_LOG=debug cargo run
cargo test google_workspace_delegated_binding -- --nocapture
```

## 必备证据包

- transaction start 结果
- callback / exchange 成功证据
- binding 与 subject metadata 落库记录
- 真实远程调用记录
- 审计链截图

## 不通过条件

- Google Workspace delegated 路径需要旁路实现。
- subject metadata 或 scope snapshot 无法落库或无法展示。
- 运行时调用成功但缺少远程与本地双证据。
