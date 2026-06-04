# ZKM-173 阶段性验收文档

## 任务

- 卡片：`ZKM-173`
- 标题：`[Vault][OAuth Broker] 落地 Binding Handle 运行时令牌与 Admin/Use 平面分离`

## 阶段目标

把当前受限 token 从 `credential_ids` 白名单语义推进到 `binding_handle` 语义，并明确 Admin / Use 平面的 claims、issued_from 与 allowed operations 边界。

## 验收前提

- `ZKM-171` 已有 binding 资源壳。
- 本地 token 生成与校验链路可运行。
- 已准备一个最小 binding 样例。

## 本地优先验证方式

- 完整本地验证即可，不依赖第三方 provider。
- 使用本地 API 调用、claims 解码、权限拒绝测试和审计日志作为主要证据。

## 验收场景

### AC-173-1 Management token 与 runtime token 的 claims 被区分

- 操作：
  - 生成管理平面 token 与运行时 token。
  - 对比 `subject_type`、`issued_from`、资源限制字段。
- 期望结果：
  - 两类 token 的语义、来源和允许动作可被稳定识别。
- 关键证据：
  - token 创建响应。
  - claims 解码结果。

### AC-173-2 运行时限制转向 binding_handle

- 操作：
  - 使用仅授权某个 binding handle 的 runtime token 访问合法和非法资源。
- 期望结果：
  - 合法 handle 可通过，未授权 handle 被明确拒绝。
- 关键证据：
  - 成功与拒绝的 API 响应。
  - 审计日志中的资源标识。

### AC-173-3 UI / API / 审计都能表达平面边界

- 操作：
  - 检查 token 列表、developer 页面或 API 是否展示 token 类型和绑定范围。
- 期望结果：
  - 用户和开发者能看清 token 适用边界，不再只有模糊的受限 token 语义。
- 关键证据：
  - 前端截图。
  - API 元数据响应。

## 建议执行命令

```bash
cargo test access_token -- --nocapture
cargo test binding_handle -- --nocapture
rtk rg -n "credential_ids|binding_handle|issued_from|subject_type" src frontend
```

## 必备证据包

- token claims 解码结果
- 允许 / 拒绝调用样本
- 审计日志记录
- 页面截图或 API 元数据输出

## 不通过条件

- 运行时仍只能按 `credential_ids` 授权。
- 管理 token 和运行时 token 在 claims 上无法区分。
- 平台没有对未授权 binding handle 做稳定拒绝。
