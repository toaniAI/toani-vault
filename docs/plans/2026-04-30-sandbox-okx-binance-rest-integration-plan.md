# Sandbox OKX / Binance REST 集成实现计划

日期：2026-04-30

## 1. 背景

目标是在当前系统的 `sandbox` 中支持 OKX 和 Binance 的程序化 REST API 调用能力，复用现有 `sandbox execute http_request` 机制，通过凭证字段、模板函数和域名白名单来完成受控请求。

本期不支持 WebSocket。

## 2. 范围

### 2.1 本期范围

- 仅支持 `REST API`
- 通过现有 `sandbox execute http_request` 完成请求
- 支持 `OKX`、`Binance`、`custom` 三种 provider
- 支持凭证固定字段 `api_key`、`secret_key`
- 支持独立明文配置字段：
  - `provider`
  - `allowed_domains`
  - `custom_functions`
- 支持在请求这些位置进行模板渲染：
  - URL
  - Query 参数
  - Headers
  - Body
- 支持两类模板引用：
  - `${credential.api_key}`
  - `${functions.xxx(...)}`
- 支持 `TypeScript` 自定义函数
- 支持 `OKX`、`Binance` 预制模板函数

### 2.2 非范围

- WebSocket 建连、转发、鉴权
- 相对路径 URL 组装
- 自动签名模式
- 自定义函数返回对象、数组或任意 JSON

## 3. 最终需求约束

### 3.1 凭证模型

固定敏感字段仍放在加密 `plaintext_data` 中：

```json
{
  "api_key": "xxx",
  "secret_key": "yyy",
  "passphrase": "zzz"
}
```

说明：

- `api_key`、`secret_key` 为固定字段
- `OKX` 额外要求 `passphrase`
- `passphrase` 作为自定义字段存放在加密内容中

### 3.2 长期配置

以下字段单独存储，且不加密：

- `provider`
- `allowed_domains`
- `custom_functions`

### 3.3 调用时临时参数

以下字段不存储在 Credential 中，按次传入：

- `url`
- `headers`
- `body`
- 其他 `http_request` 运行参数

### 3.4 URL 规则

- 仅允许绝对 URL
- 不支持相对路径
- 请求前必须做 `allowed_domains` 校验

### 3.5 白名单规则

- 支持子域名通配
- 包含端口校验
- 不区分协议
- 允许：
  - `https://www.okx.com`
  - `wss://ws.okx.com`
- 拒绝：
  - `https://evil-okx.com`
  - `https://www.okx.com.evil.com`

建议内部匹配语义为：

- 以 `host + port` 为核心匹配单元
- 忽略 `scheme`
- 支持 `*.example.com:443`

### 3.6 模板函数规则

- `custom_functions.function_body` 必须是 `TypeScript`
- 顶部依赖注解固定格式：

```ts
/* @imports: crypto-js@4.2.0, qs@6.13.0 */
```

- 允许任意 npm 包
- Node 内置模块默认加载
- 函数返回值仅允许字符串
- 函数可访问：
  - 凭证字段变量，如 `api_key`、`secret_key`
  - 调用参数
  - 请求上下文变量，如 `method`、`url`、`query`、`body`
- 函数禁止：
  - 子进程
  - 环境变量
  - 文件读写
  - 网络请求

### 3.7 Provider 模板策略

本期采用半自动模板：

- 系统提供预制函数
- 调用方仍需在 `headers`、`query`、`body` 中显式使用 `${functions.xxx(...)}`

## 4. 官方协议约束

### 4.1 OKX

根据官方文档，OKX 私有 REST 请求要求：

- `OK-ACCESS-KEY`
- `OK-ACCESS-SIGN`
- `OK-ACCESS-TIMESTAMP`
- `OK-ACCESS-PASSPHRASE`

签名规则为：

- 对 `timestamp + method + requestPath + body` 做 `HMAC-SHA256`
- 输出 `Base64`

参考：

- [OKX API Docs](https://www.okx.com/docs-v5/en)

### 4.2 Binance Spot

根据官方文档，Binance Spot REST `SIGNED` 请求要求：

- `X-MBX-APIKEY`
- `timestamp`
- `signature`

签名规则为：

- 对最终参数串做 `HMAC-SHA256`
- 输出十六进制字符串

本期约定：

- 调用方自己在 query 或 body 中放 `${functions.binance_timestamp()}`
- 调用方自己再把 `${functions.binance_sign()}` 放入 `signature`
- 系统从最终渲染后的 query/body 提取签名源串

参考：

- [Binance Spot Request Security](https://developers.binance.com/docs/binance-spot-api-docs/rest-api/request-security)

## 5. 建议的数据结构

### 5.1 Credential 独立配置

```json
{
  "provider": "okx",
  "allowed_domains": [
    "*.okx.com:443",
    "www.okx.com:443"
  ],
  "custom_functions": [
    {
      "function_name": "build_auth_header",
      "function_description": "build bearer header",
      "function_body": "/* @imports: crypto-js@4.2.0 */\nexport default function func(input) { return `Bearer ${api_key}:${input}`; }"
    }
  ]
}
```

### 5.2 OKX 请求示例

```json
{
  "method": "GET",
  "url": "https://www.okx.com/api/v5/account/balance",
  "headers": {
    "OK-ACCESS-KEY": "${credential.api_key}",
    "OK-ACCESS-TIMESTAMP": "${functions.okx_timestamp()}",
    "OK-ACCESS-PASSPHRASE": "${credential.passphrase}",
    "OK-ACCESS-SIGN": "${functions.okx_sign()}",
    "Content-Type": "application/json"
  }
}
```

### 5.3 Binance 请求示例

```json
{
  "method": "GET",
  "url": "https://api.binance.com/api/v3/account?timestamp=${functions.binance_timestamp()}&signature=${functions.binance_sign()}",
  "headers": {
    "X-MBX-APIKEY": "${credential.api_key}"
  }
}
```

## 6. 实现设计

### 6.1 数据层改造

需要扩展当前 Credential 存储结构。

涉及文件：

- `/Users/yvan/AIWorkspace/credbridge/src/services/db/schema.rs`
- `/Users/yvan/AIWorkspace/credbridge/src/vault/models.rs`
- `/Users/yvan/AIWorkspace/credbridge/src/vault/postgres.rs`

建议新增字段：

- `provider VARCHAR(32) NULL`
- `allowed_domains JSONB NULL`
- `custom_functions JSONB NULL`

设计要求：

- 与现有 `encrypted_payload` 解耦
- 允许旧凭证无这些字段
- 更新与查询时完整返回

### 6.2 后端 Credential API 改造

涉及文件：

- `/Users/yvan/AIWorkspace/credbridge/src/api/credentials.rs`

需要扩展：

- `CreateCredentialApiRequest`
- `CreateCredentialResponse`
- `UpdateCredentialApiRequest`
- `UpdateCredentialApiResponse`
- Credential 详情接口响应

新增验证：

- `provider` 仅允许 `okx|binance|custom`
- `allowed_domains` 格式合法
- `custom_functions` 结构合法
- `function_name` 唯一
- `function_body` 必须存在
- `@imports` 注解格式可解析

### 6.3 前端凭证配置页改造

涉及文件：

- `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/credentials/pages/CredentialsPage.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/api/types.ts`

需要新增 UI：

- `provider` 下拉选择
- `allowed_domains` 列表编辑
- `custom_functions` 编辑器

额外要求：

- `provider=okx` 时提示需要 `passphrase`
- 对 `function_body` 提供代码输入区域
- 对 `allowed_domains` 提供即时格式提示

### 6.4 Sandbox HTTP 模板层

在 `src/tee/sandbox/` 下新增模块，建议拆分为：

- `http_template.rs`
- `function_runtime.rs`
- `domain_policy.rs`

职责如下：

- 读取 session 绑定的 Credential 明文与独立配置
- 执行 URL 白名单校验
- 渲染 `${credential.xxx}`
- 渲染 `${functions.xxx(...)}`
- 把最终参数交给现有 `http_request` 执行器

### 6.5 Session 凭证缓存扩展

当前 session 只缓存可委托标量字段，需要扩展为同时缓存：

- 解密后的凭证字段
- `provider`
- `allowed_domains`
- `custom_functions`

涉及文件：

- `/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs`
- `/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/session.rs`

### 6.6 预制函数实现

建议内置以下函数：

#### OKX

- `okx_timestamp()`
  - 返回 ISO8601 毫秒 UTC 时间字符串
- `okx_sign()`
  - 从请求上下文读取：
    - `method`
    - `url`
    - `query`
    - `body`
  - 从凭证读取：
    - `secret_key`
    - `passphrase`
  - 输出 Base64 字符串

#### Binance

- `binance_timestamp()`
  - 返回毫秒时间戳字符串
- `binance_sign()`
  - 从最终 query/body 提取参数串
  - 使用 `secret_key` 做 `HMAC-SHA256`
  - 输出 hex 字符串

#### Custom

- 不提供预制函数

### 6.7 TypeScript 函数运行时

这是本需求中风险最高的部分，建议独立封装。

推荐实现：

- 使用受控 Node worker 作为函数执行器
- 单次函数执行设置：
  - 超时
  - 内存限制
  - 输出限制
- 依赖按 `@imports` 下载并缓存
- 拦截危险能力：
  - `child_process`
  - `fs`
  - `process.env`
  - 网络访问

运行时接口建议：

- 输入：
  - Credential 字段
  - 函数参数
  - 请求上下文
- 输出：
  - 单个字符串

### 6.8 现有 `http_request` 的接入方式

现有 `http_request` 已支持：

- `method`
- `url`
- `headers`
- `body`
- `timeout_ms`

本期只在执行前增加一层模板预处理，不改动基础发送机制。

涉及文件：

- `/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/session.rs`
- `/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs`
- `/Users/yvan/AIWorkspace/credbridge/cli/src/commands/sandbox.ts`

### 6.9 CLI / SDK / 文档同步

需要同步修改：

- `/Users/yvan/AIWorkspace/credbridge/cli/SKILL.md`
- `/Users/yvan/AIWorkspace/credbridge/cli/README.md`
- `/Users/yvan/AIWorkspace/credbridge/API.md`
- `sdk-rust`
- `sdk-typescript`

至少补充：

- 创建带 provider 配置的 Credential 示例
- OKX 查询余额示例
- Binance 查询账户示例

## 7. 推荐实施顺序

1. 先改数据库 schema 和后端数据模型
2. 再改 Credential CRUD API
3. 再改前端 Credential 配置页
4. 实现域名白名单校验
5. 实现模板渲染器
6. 实现 OKX / Binance 预制函数
7. 实现 TypeScript 函数运行时
8. 接入现有 `http_request`
9. 更新 CLI / SDK / 文档
10. 补全测试并验证

## 8. 测试计划

### 8.1 单元测试

- `provider` 字段校验
- `allowed_domains` 解析与匹配
- 模板语法解析
- `@imports` 注解解析
- OKX 签名结果
- Binance 签名结果
- 函数返回非字符串时报错

### 8.2 集成测试

- 创建带配置的 API Key Credential
- 更新 Credential 独立配置
- Session 通过 `service_id` 绑定 Credential
- `http_request` 对白名单 URL 放行
- `http_request` 对非白名单 URL 拒绝
- `OKX` 缺少 `passphrase` 时失败
- `Binance` 签名参数提取正确

### 8.3 安全测试

- 禁止访问 `fs`
- 禁止访问 `child_process`
- 禁止读取 `process.env`
- 禁止网络调用
- 禁止通过动态 import 绕过限制

## 9. 验收标准

- 能创建带 `provider`、`allowed_domains`、`custom_functions` 的 `api_key` Credential
- `sandbox execute http_request` 能渲染 `${credential.xxx}`
- `sandbox execute http_request` 能渲染 `${functions.xxx(...)}`
- 对非白名单域名请求直接拦截
- OKX 模板能完成私有 REST 请求签名
- Binance 模板能完成 `SIGNED` REST 请求签名
- `custom_functions` 能读取凭证字段和请求上下文
- `custom_functions` 无法访问文件、环境变量、网络和子进程
- 现有非交易所 `http_request` 不回归

## 10. 风险

### 10.1 依赖供应链风险

允许任意 npm 包会带来明显供应链风险，需要：

- 严格隔离执行环境
- 对下载和缓存做审计
- 控制安装和执行权限

### 10.2 运行时复杂度

TypeScript 函数执行器引入：

- 依赖安装
- 版本缓存
- 资源限制
- 安全隔离

建议将其作为独立可测试组件实现。

### 10.3 交易所签名细节敏感

- OKX 对 `requestPath + body` 拼接敏感
- Binance 对参数串顺序和编码敏感

必须在实现中固定渲染顺序和签名输入规则。

## 11. 最终验证步骤

实现完成后，最终验证必须分为“静态校验”和“真实接口联调验证”两层。

### 11.1 静态校验

至少执行以下检查：

```bash
rtk cargo fmt
rtk cargo clippy --tests -- -D warnings
rtk cargo test
```

如涉及前端变更，还应补充：

```bash
cd frontend
rtk npm run lint
rtk npm run build
```

### 11.2 真实接口联调验证

除静态校验外，还必须使用真实可用的测试凭证，分别通过该功能完成一次 `OKX` 和 `Binance` 私有 REST 接口调用，证明功能不是“签名算法看起来正确”，而是“接口实际可用”。

建议验证方式如下：

1. 准备两个 `api_key` 类型 Credential：
   - `provider=okx`，包含 `api_key`、`secret_key`、`passphrase`
   - `provider=binance`，包含 `api_key`、`secret_key`
2. 为两个 Credential 分别配置精确的 `allowed_domains`：
   - OKX：`www.okx.com:443` 或 `*.okx.com:443`
   - Binance：`api.binance.com:443`
3. 创建绑定对应 Credential 的 sandbox session。
4. 通过 `sandbox execute http_request` 发起真实私有 REST 请求，不允许绕过模板层直接在外部预签名后再传入固定 header 或 signature。
5. 记录请求结果，至少包括：
   - HTTP 状态码
   - 交易所返回的业务 `code` / `msg` / `success` 字段
   - 关键响应字段是否存在
6. 验证成功后，再补一组失败用例，确认错误路径也符合预期：
   - 移除或篡改签名后请求失败
   - 请求非白名单域名时被本系统直接拦截
   - OKX 缺少 `passphrase` 时失败

建议最小联调用例如下：

#### OKX 验证

- 使用账户类只读私有接口，例如：
  - `GET https://www.okx.com/api/v5/account/balance`
- 请求中必须通过模板生成以下头：
  - `OK-ACCESS-KEY`
  - `OK-ACCESS-TIMESTAMP`
  - `OK-ACCESS-PASSPHRASE`
  - `OK-ACCESS-SIGN`
- 成功标准：
  - 请求返回 HTTP `200`
  - 返回体中 OKX 业务成功码正确
  - 返回体中存在账户余额相关字段，说明私有接口鉴权成功

#### Binance 验证

- 使用账户类只读私有接口，例如：
  - `GET https://api.binance.com/api/v3/account`
- 请求中必须通过模板生成：
  - Query 中的 `timestamp`
  - Query 或 body 中的 `signature`
  - Header `X-MBX-APIKEY`
- 成功标准：
  - 请求返回 HTTP `200`
  - 返回体未出现签名错误、时间戳错误或鉴权失败错误
  - 返回体中存在账户信息字段，说明 `SIGNED` 请求已被 Binance 正确接受

只有在以下条件同时满足时，才算最终验收通过：

- 静态检查全部通过
- 自动化测试全部通过
- 能通过该功能成功调用一次 OKX 私有 REST 接口
- 能通过该功能成功调用一次 Binance 私有 REST 接口
- 白名单拦截与签名失败路径验证通过

## 12. 备注

当前仓库中的已有相关实现入口包括：

- Sandbox API：`/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs`
- Sandbox Session：`/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/session.rs`
- Credential API：`/Users/yvan/AIWorkspace/credbridge/src/api/credentials.rs`
- Credential Schema：`/Users/yvan/AIWorkspace/credbridge/src/services/db/schema.rs`
- 前端凭证页：`/Users/yvan/AIWorkspace/credbridge/frontend/src/features/credentials/pages/CredentialsPage.tsx`

该计划可直接作为后续开发分解依据。
