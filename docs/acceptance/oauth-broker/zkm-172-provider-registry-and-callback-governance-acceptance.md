# ZKM-172 阶段性验收文档

## 任务

- 卡片：`ZKM-172`
- 标题：`[Vault][OAuth Broker] 落地 Provider Registry 校验与 Callback Adapter 治理`

## 阶段目标

建立受控 Provider Registry、配置验证能力与 Callback Adapter 治理机制，使 OAuth 配置在正式授权前即可被验证，并且 provider-specific 适配逻辑不能绕过核心安全门。

## 验收前提

- `ZKM-171` 资源壳已落地。
- 本地后端可使用测试 provider 定义运行。
- 若涉及网络探测，已准备好非生产 endpoint 或 mock endpoint。

## 本地优先验证方式

- 优先使用本地 mock endpoint、伪造 callback 参数和本地测试数据验证。
- 仅在必要时补充真实远程 endpoint 的连通性记录。

## 验收场景

### AC-172-1 Provider definition 可受控管理

- 操作：
  - 本地创建、查看、更新一条 provider definition。
  - 验证其版本与适用信息是否可读。
- 期望结果：
  - provider 元数据进入受控注册表，而不是散落在配置文件或硬编码中。
- 关键证据：
  - API / CLI 输出。
  - 数据库存储结果。

### AC-172-2 OAuth 配置验证能给出结构化结果

- 操作：
  - 分别构造成功与失败的 provider 配置。
  - 执行本地验证接口。
- 期望结果：
  - 至少覆盖 endpoint、client auth、callback mode、adapter 绑定与基础连通性。
- 关键证据：
  - 成功与失败的验证响应。
  - 错误原因字段截图。
  - 若涉及探测，请保留请求日志。

### AC-172-3 Callback adapter 无法绕过核心校验

- 操作：
  - 构造 transaction 不存在、state 不匹配、重复消费等场景。
  - 验证 adapter 不会在核心校验失败后继续推进流程。
- 期望结果：
  - adapter 只做标准化与映射，不改变核心安全判定。
- 关键证据：
  - 本地测试日志。
  - 审计事件顺序记录。
  - 回调失败响应截图。

## 建议执行命令

```bash
cargo test provider_registry -- --nocapture
cargo test callback_adapter -- --nocapture
rtk rg -n "callback|provider|validate" src tests
```

## 必备证据包

- provider definition 创建与查询响应
- 配置验证成功/失败样本
- callback 核心校验优先的测试或日志证据

## 不通过条件

- provider 仍需靠人工记忆或硬编码配置。
- 验证能力不能明确指出失败原因。
- adapter 在 transaction/state 失败场景下仍能推进流程。
