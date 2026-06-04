# ZKM-171 阶段性验收文档

## 任务

- 卡片：`ZKM-171`
- 标题：`[Vault][OAuth Broker] 落地 Broker 资源壳与管理面清单`

## 阶段目标

把系统从“raw credential 为中心”推进到“binding 为中心”的一等资源模型，并交付最小管理面闭环。

## 验收前提

- `ZKM-162` 已输出边界与命名结论。
- 本地 PostgreSQL、Redis、immudb、Vault 可启动。
- 本地后端可在 `TEE_MODE=simulation` 下运行。

## 本地优先验证方式

- 通过本地数据库迁移、后端 API、前端页面和集成测试完成主要验证。
- 不依赖真实第三方 provider 即可完成本阶段验收。

## 验收场景

### AC-171-1 一等资源模型已落库

- 操作：
  - 执行本地迁移或启动流程。
  - 查询 binding、provider、policy snapshot、runtime state 相关表或结构。
- 期望结果：
  - 核心资源存在稳定持久化结构，并带有 tenant 作用域。
- 关键证据：
  - 迁移日志。
  - 数据库结构查询结果。
  - 关键实体样例数据截图。

### AC-171-2 管理平面可 create / list / inspect

- 操作：
  - 通过本地 API 或 CLI 调用最小管理面接口。
  - 创建一条测试 provider / binding 资源并查询详情。
- 期望结果：
  - create、list、inspect 路径完整可用，且返回结构化状态。
- 关键证据：
  - 请求/响应报文。
  - 审计日志记录。
  - 失败场景的结构化报错。

### AC-171-3 前端可展示 binding-centered inventory

- 操作：
  - 打开本地前端页面。
  - 检查 inventory、详情或 developer 页面是否展示 binding 视图。
- 期望结果：
  - 用户能从 UI 看到 binding-centered 的模型，而不是只能看到底层凭证。
- 关键证据：
  - 页面截图。
  - 对应前端 API 调用记录。

## 建议执行命令

```bash
docker compose -f docker/docker-compose.yml up -d postgres redis immudb vault
TEE_MODE=simulation RUST_LOG=debug cargo run
cargo test broker_resource -- --nocapture
psql "$DATABASE_URL" -c "\\dt"
```

## 必备证据包

- 数据库结构查询结果
- API create/list/inspect 响应
- 前端截图
- 审计事件样本

## 不通过条件

- 资源模型仍然停留在内存或临时结构。
- 管理平面只有写入能力，没有 list/inspect 闭环。
- UI 无法体现 binding-centered 模型。
