# ZKM-174 阶段性验收文档

## 任务

- 卡片：`ZKM-174`
- 标题：`[Vault][OAuth Broker] 交付 Lark App 或 Tenant Token Binding 首个可执行闭环`

## 阶段目标

交付第一个非交互 provider app credential binding 闭环，用 Lark App / Tenant Token 路径证明 binding-centered broker 可以落地成真实运行时调用。

## 验收前提

- `ZKM-171`、`ZKM-172`、`ZKM-173` 已完成。
- 本地环境可以连接测试用 Lark 非生产凭据。
- 已准备最小可调用的 Lark API 目标，例如 profile、chat 或 calendar 读取接口。

## 本地优先验证方式

- 本地启动后端和前端，使用本地 API / UI 完成 binding 创建与调用。
- 远程 Lark 只作为最终调用目标，控制面与主要证据尽量留在本地。

## 验收场景

### AC-174-1 可创建并验证 Lark App / Tenant Token Binding

- 操作：
  - 使用本地管理平面创建 binding。
  - 触发配置校验与可用性验证。
- 期望结果：
  - binding 进入 ready 或等价的可用状态，并拥有稳定 handle。
- 关键证据：
  - 创建响应。
  - 验证响应。
  - 数据库存储结果。

### AC-174-2 基于 binding handle 完成一次真实 governed invocation

- 操作：
  - 使用运行时 token 调用已批准的 Lark API。
- 期望结果：
  - 平台返回脱敏业务结果，不暴露长期 secret。
- 关键证据：
  - 本地运行时请求/响应。
  - 远程 Lark 调用记录或请求 ID。
  - 本地审计事件。

### AC-174-3 错误路径具备可解释结果

- 操作：
  - 构造错误 scope、无效 app token 或不允许的目标域名。
- 期望结果：
  - 返回结构化错误，而不是模糊失败。
- 关键证据：
  - 失败响应样本。
  - 本地日志。

## 建议执行命令

```bash
TEE_MODE=simulation RUST_LOG=debug cargo run
cargo test lark_binding -- --nocapture
```

## 必备证据包

- binding 创建与验证响应
- 真实远程调用记录
- 本地审计日志
- 页面截图或 CLI 输出

## 不通过条件

- binding 只能存储，不能实际调用。
- 运行时输出泄露 token 或长期 secret。
- 成功调用缺少本地与远程双侧证据。
