# ZKM-176 阶段性验收文档

## 任务

- 卡片：`ZKM-176`
- 标题：`[Vault][OAuth Broker] 交付 Lark Delegated User Binding 闭环`

## 阶段目标

交付第一个完整的交互式 delegated OAuth 闭环，覆盖 transaction start、platform callback、TEE 内 code exchange、binding ready 和后续 governed invocation。

## 验收前提

- `ZKM-171`、`ZKM-172`、`ZKM-173` 已完成。
- 已准备可用于测试的 Lark delegated OAuth 配置。
- 若本地 callback 无法直接暴露，已准备开发回调域名或本地隧道方案。

## 本地优先验证方式

- 优先在本地完成 transaction、callback、状态迁移和运行时调用验证。
- 若浏览器回调需要开发域名，只把最小前通道暴露出去，其余日志、状态和证据仍在本地采集。

## 验收场景

### AC-176-1 可启动 delegated auth transaction

- 操作：
  - 本地发起 transaction start。
  - 检查授权链接、state、PKCE、过期时间和 transaction 状态。
- 期望结果：
  - transaction 被持久化且可追踪。
- 关键证据：
  - start 响应。
  - transaction 数据落库结果。

### AC-176-2 platform callback 与 code exchange 闭环成立

- 操作：
  - 完成一次真实或受控的授权回调。
  - 检查核心校验、TEE 内 code exchange 与 binding 生成。
- 期望结果：
  - callback 消费一次成功，再次消费会被拒绝。
  - 成功后生成可用 binding。
- 关键证据：
  - 浏览器回调截图。
  - 本地回调处理日志。
  - token exchange 结果与 binding 落库记录。

### AC-176-3 新 binding 可驱动一次受控调用

- 操作：
  - 使用新生成的 binding handle 完成一次目标 API 调用。
- 期望结果：
  - 运行时输出为脱敏业务结果，审计链路完整。
- 关键证据：
  - 运行时响应。
  - 审计事件链。
  - 远程调用记录。

## 建议执行命令

```bash
TEE_MODE=simulation RUST_LOG=debug cargo run
cargo test lark_delegated_binding -- --nocapture
```

## 必备证据包

- transaction start 响应
- callback 成功与 replay 拒绝证据
- binding 创建记录
- 真实运行时调用记录
- 审计链截图

## 不通过条件

- transaction 无法持久化或不可追踪。
- callback replay 无法被拒绝。
- code exchange 在 TEE 外完成或缺少证据。
