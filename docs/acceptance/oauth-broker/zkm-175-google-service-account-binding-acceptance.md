# ZKM-175 阶段性验收文档

## 任务

- 卡片：`ZKM-175`
- 标题：`[Vault][OAuth Broker] 交付 Google Service Account Binding 首个可执行闭环`

## 阶段目标

交付第二个非交互 binding 闭环，用 Google Service Account 路径验证 broker 核心不是单 provider 特化实现。

## 验收前提

- `ZKM-171`、`ZKM-172`、`ZKM-173` 已完成。
- 已准备非生产 Google service account 和最小可读 API 目标。
- 本地环境可访问对应 Google API。

## 本地优先验证方式

- 控制面、运行时和证据收集在本地完成。
- 真实 Google API 只用于最终远程调用证据。

## 验收场景

### AC-175-1 Service account binding 可创建并验证

- 操作：
  - 在本地创建 Google service account binding。
  - 校验证书/凭据可用性和 scope 配置。
- 期望结果：
  - binding 进入可用状态并生成稳定 handle。
- 关键证据：
  - 创建与验证响应。
  - 数据落库记录。

### AC-175-2 运行时可完成一次真实 Google API 调用

- 操作：
  - 通过 binding handle 发起一次目标 API 调用。
- 期望结果：
  - 平台只返回业务结果，不返回可复用 token。
- 关键证据：
  - 本地调用报文。
  - 远程调用记录或 provider request id。
  - 审计日志。

### AC-175-3 scope 与主体语义可被验证

- 操作：
  - 使用过宽、错误或缺失 scope 的配置重试。
- 期望结果：
  - 返回结构化拒绝，且可区分配置错误与远程 provider 异常。
- 关键证据：
  - 失败响应。
  - 本地调试日志。

## 建议执行命令

```bash
TEE_MODE=simulation RUST_LOG=debug cargo run
cargo test google_service_binding -- --nocapture
```

## 必备证据包

- binding 创建 / 验证结果
- 真实远程调用记录
- scope 失败样本
- 审计日志

## 不通过条件

- Google 路径仍依赖临时脚本或旁路逻辑。
- 调用结果返回 token 或密钥材料。
- 无法区分 scope 问题与 provider 问题。
