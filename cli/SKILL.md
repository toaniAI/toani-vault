# Toani Vault CLI Skill

## 目标

给 agent 一份可直接执行的 `toani` 使用规范，重点覆盖：

- 当前真实可用命令面
- 标准 `sandbox` 调用链
- `execute_script.bindings` 和 secret 消费的真实语义
- 常见操作的最小可执行示例

## 核心心智模型

1. `toani` 是 CLI，不是 SDK 伪代码。
2. `sandbox` 是 CredBridge 后端提供的远端 TEE 浏览器会话，不是本地浏览器，也不是 agent 自己的运行时节点。
3. 页面操作必须通过 `toani sandbox create-session` 和 `toani sandbox execute`。
4. `http_request` 是后端直连 HTTP，不会启动远端浏览器。
5. 需要页面状态时，优先用 `get-session`、`get-operation`、`export-dom`、`execute_script`。
6. 完成后要 `terminate`，不要留下长期活跃会话。

## 当前真实能力面

先信当前 CLI 实现和 `toani --help`，不要信旧文档。

当前公开命令组只有：

- `config`
- `sandbox`
- `--help`
- `--version`

不要默认存在这些命令组，除非你先验证过：

- `auth`
- `credentials`
- `tokens`
- `service-accounts`
- `audit`

## 全局参数与优先级

全局参数：

- `--output json|table`
- `--base-url <URL>`
- `--token <TOKEN>`

Base URL 优先级：

1. `--base-url`
2. `TOANI_BASE_URL`
3. `CREDBRIDGE_BASE_URL`
4. `config.baseUrl`
5. 默认 `https://api.credbridge.example/`

Token 优先级：

1. `--token`
2. `config.token`
3. `TOANI_VAULT_TOKEN`
4. `CREDBRIDGE_TOKEN`

注意：

- `--base-url`、`--token`、`--output` 会写回 `~/.toani/config.json`
- 自动化默认用 `--output json`
- 不要把 token 打进日志或提交到仓库

## 标准初始化

```bash
toani --help
toani config show --output json
toani sandbox create-session --help
toani sandbox execute --help
```

如果用户只给了“凭证名”但没给 `credential_id`，先核对当前环境和 token，再请求 CredBridge API 或控制面查真实凭证 ID，不要猜。

## Sandbox 命令表

```bash
toani sandbox create-session --service-id <serviceId> --original-intent <intent> [--credential-id <id>] [--start-url <url>]
toani sandbox list-sessions
toani sandbox get-session <sessionId>
toani sandbox terminate <sessionId>
toani sandbox pause <sessionId>
toani sandbox resume <sessionId>
toani sandbox execute <sessionId> --operation-type <type> [--params '<json>']
toani sandbox export-dom <sessionId> [--format html|text|json] [--root-selector body]
toani sandbox export-data <sessionId> --selectors '<json-array>' [--format json|csv|pdf]
toani sandbox get-operation <operationId>
toani sandbox stats
```

## `execute` 支持的 operation type

浏览器操作：

- `navigate`
- `click`
- `fill`
- `get_text`
- `execute_script`
- `wait`
- `export`
- `dom_export`

非浏览器操作：

- `http_request`

不要使用这些旧操作名：

- `get_attribute`
- `screenshot`

## `--params` 常见字段

根据操作类型组合使用：

- `url`
- `selector`
- `value`
- `script`
- `bindings`
- `timeout_ms`
- `milliseconds`
- `duration_ms`
- `method`
- `headers`
- `body`
- `selectors`
- `sensitive`

如果通过 `execute --operation-type dom_export` 调用，使用后端字段名：

- `root_selector`
- `format`
- `include_text`
- `include_metadata`
- `extra_sensitive_selectors`
- `max_bytes`

## 标准调用流程

当用户要“在 TEE 浏览器里打开页面并操作”时，按这个顺序：

1. `toani --help`
2. `toani config show --output json`
3. 如需凭证 ID，先查询并确认凭证
4. `toani sandbox create-session ...`
5. `toani sandbox execute ...`
6. 如有异步返回，再 `toani sandbox get-operation <operationId>`
7. 需要确认状态时，`toani sandbox get-session <sessionId>`
8. 完成后 `toani sandbox terminate <sessionId>`

不要跳过 `create-session`。

## Secret 使用规则

### 先区分“拿到凭证 ID”和“在沙盒里消费 secret”

这两个动作不是一回事：

1. 先在控制面或 API 列表里拿到 `credential_id`
2. 再在 `toani sandbox create-session --credential-id ...` 中把该凭证绑定到 TEE 会话
3. 后续执行 `fill` 时，后端才会在沙盒内部按需解析并消费对应字段

重要：

- `credential_id` 只是凭证引用，不是明文
- 正确路径不是“先在本地解密再传进浏览器”，而是“先绑定 session，再在 TEE 内通过受控宿主操作消费”

### `fill` 中引用凭证

这是允许且推荐的 secret 消费方式：

```bash
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'
```

### `execute_script` 中的 `bindings`

这是当前最关键的真实语义：

- `bindings` 只允许普通字符串
- `bindings` 不支持 `{"$credential":"..."}`
- secret 不允许进入 `execute_script` 的脚本上下文

正确示例：

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return document.querySelector(bindings.selector)?.getAttribute(\"href\") ?? null","bindings":{"selector":"a.download"}}'
```

错误示例：

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return bindings.password","bindings":{"password":{"$credential":"password"}}}'
```

预期结果：

- 后端直接拒绝请求
- 错误文案说明 `execute_script.bindings` 只支持普通字符串
- 不应把 secret 明文回显到结果或错误中

## 常用示例列表

### 示例 1：初始化配置

```bash
toani config init --url https://api.example.com --token <BEARER_TOKEN>
toani config show --output json
```

### 示例 2：创建 sandbox 会话

```bash
toani sandbox create-session \
  --service-id svc_example \
  --original-intent "Open target page in TEE sandbox" \
  --start-url "https://target-site.com/login"
```

### 示例 3：创建带凭证的 sandbox 会话

```bash
toani sandbox create-session \
  --service-id svc_example \
  --credential-id <credentialId> \
  --original-intent "Login with credential-backed session" \
  --start-url "https://target-site.com/login"
```

### 示例 4：导航

```bash
toani sandbox execute <sessionId> \
  --operation-type navigate \
  --params '{"url":"https://target-site.com/login"}'
```

### 示例 5：点击按钮

```bash
toani sandbox execute <sessionId> \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'
```

### 示例 6：填充普通文本

```bash
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":"user@example.com"}'
```

### 示例 7：填充凭证字段

```bash
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'
```

### 示例 8：等待元素出现

```bash
toani sandbox execute <sessionId> \
  --operation-type wait \
  --params '{"selector":"#dashboard","timeout_ms":10000}'
```

### 示例 9：读取文本

```bash
toani sandbox execute <sessionId> \
  --operation-type get_text \
  --params '{"selector":"h1"}'
```

### 示例 10：执行脚本读取属性或文本

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return document.querySelector(bindings.selector)?.textContent?.trim() ?? null","bindings":{"selector":"h1"}}'
```

### 示例 11：执行脚本读取页面状态

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return { href: location.href, title: document.title }"}'
```

### 示例 12：导出脱敏 DOM

```bash
toani sandbox export-dom <sessionId> \
  --format html \
  --root-selector body \
  --include-text true \
  --include-metadata true \
  --extra-sensitive-selectors '["#token",".secret"]'
```

### 示例 13：后端直连 HTTP 请求

```bash
toani sandbox execute <sessionId> \
  --operation-type http_request \
  --params '{"url":"https://api.example.com/status","method":"GET","timeout_ms":10000}'
```

### 示例 14：批量导出选择器文本

```bash
toani sandbox execute <sessionId> \
  --operation-type export \
  --params '{"selectors":["h1",".status"]}'
```

### 示例 15：导出结构化数据

```bash
toani sandbox export-data <sessionId> \
  --selectors '["h1",".status"]' \
  --format json
```

### 示例 16：查询异步操作结果

```bash
toani sandbox get-operation <operationId>
```

### 示例 17：查询会话状态

```bash
toani sandbox get-session <sessionId>
```

### 示例 18：结束会话

```bash
toani sandbox terminate <sessionId>
```

## 明确禁止的误用

下面这些都不对：

- 把 TEE sandbox 理解成 agent 自己的 nodes、graph、workflow node
- 在没有 `sessionId` 的情况下直接执行 `sandbox execute`
- 发明 CLI 没有实现的命令组
- 把 `sandbox` 当成本地 Playwright、浏览器 devtools 或 OpenClaw 内置网页操作器
- 使用旧操作 `get_attribute` 或 `screenshot`
- 让 secret 进入 `execute_script.bindings`
- 用自然语言描述代替具体命令和参数

## 常见错误与修复

- 错误：`Unknown command group`
  - 修复：先执行 `toani --help`，不要使用旧文档里的未发布命令组

- 错误：`Usage: toani sandbox create-session ...` 或缺少 `--service-id`、`--original-intent`
  - 修复：`create-session` 这两个参数必填

- 错误：`Usage: toani sandbox execute <sessionId> --operation-type <type>`
  - 修复：`execute` 必须显式传 `sessionId` 和 `--operation-type`

- 错误：`Invalid JSON for --params`
  - 修复：`--params` 必须是合法 JSON 字符串，推荐单引号包裹整段 JSON

- 错误：401/403
  - 修复：检查 `--token`、环境变量和 `~/.toani/config.json` 的优先级覆盖关系

- 错误：连到错误环境
  - 修复：显式传 `--base-url`，再用 `toani config show` 确认

- 错误：后端返回不支持的 `operation_type`
  - 修复：对照本文件的 `operation type` 列表；不要使用 `get_attribute`、`screenshot`

- 错误：`execute_script.bindings` 中传了 `{"$credential":"..."}`
  - 修复：把 secret 消费改成顶层 `fill.value` 这类受控操作；脚本 binding 只传普通字符串

- 错误：`browser runtime closed without response`、`lightpanda`、`puppeteer-core`、`CDP`、`nsjail` 相关报错
  - 修复：这是远端 Lightpanda 运行时或隔离策略问题，不是本地 CLI 浏览器问题；保留 `operationId`，执行 `toani sandbox get-operation <operationId>`，再把 session、operation、base URL 和报错交给后端排查

## 安全注意事项

- 本地配置文件路径：`~/.toani/config.json`
- 配置文件可能包含 token，禁止提交到仓库
- 自动化优先用环境变量注入 token
- 不要把 bearer token 写进长期保存的脚本、截图、日志
