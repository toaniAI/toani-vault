# ZKM-178 阶段性验收文档

## 任务

- 卡片：`ZKM-178`
- 标题：`[Vault][OAuth Broker] 落地运行时硬化与生命周期治理`

## 阶段目标

把前续 broker 交付提升到可运营、可审计、可稳定回归的状态，覆盖 refresh、cooldown、`rebind_required`、revoke / delete、staged change 与统一错误契约。

## 验收前提

- `ZKM-174` 至 `ZKM-177` 已至少在测试环境闭环。
- 本地环境能够跑完整回归测试。
- 至少具备一组非交互 binding 和一组 delegated binding 测试样本。

## 本地优先验证方式

- 优先使用本地集成测试、模拟 provider 错误和数据库状态检查。
- 真实远程调用只用于证明刷新或失败行为在真实 provider 上可复现。

## 验收场景

### AC-178-1 refresh / cooldown / rebind_required 行为成立

- 操作：
  - 构造 access token 可复用、需要 refresh、refresh 连续失败三类场景。
- 期望结果：
  - 平台能区分缓存复用、正常 refresh、进入 cooldown 和 `rebind_required`。
- 关键证据：
  - 本地日志。
  - 运行时状态落库记录。
  - 结构化错误响应。

### AC-178-2 revoke / delete / staged change 生命周期可追踪

- 操作：
  - 对 active binding 执行 revoke、delete 和 staged change + revalidate。
- 期望结果：
  - 生命周期状态变化有明确规则，且保留必要审计历史。
- 关键证据：
  - 操作前后数据库查询结果。
  - 审计日志。
  - API 响应。

### AC-178-3 回归验收套件覆盖主要成功与失败路径

- 操作：
  - 在本地执行 Lark / Google 的主要成功与失败场景测试。
- 期望结果：
  - 形成可重复执行的回归基线，能证明核心安全保证仍成立。
- 关键证据：
  - 测试报告。
  - 失败场景样本。
  - 关键截图或日志片段。

## 建议执行命令

```bash
cargo test oauth_broker -- --nocapture
cargo test refresh -- --nocapture
cargo test revoke -- --nocapture
cargo test audit -- --nocapture
```

## 必备证据包

- refresh / cooldown / rebind_required 证据
- revoke / delete / staged change 证据
- 回归测试报告
- 审计链与数据库状态截图

## 不通过条件

- 失败路径只有日志，没有结构化结果。
- 生命周期动作会抹掉关键取证信息。
- 回归测试无法证明前面 4 条 provider 路径仍然成立。
