# Toani Vault CLI Skill

## 目标

给 AI agent 一份低歧义、可直接执行的 `@toani/vault-cli` / `toani` 使用规范。

这个 skill 的重点不是解释 TEE 理论，而是告诉 agent:

- 这是一个命令行工具，入口是 `toani ...`
- 当前公开能力只有 `config` 和 `sandbox`
- 所谓 sandbox 是远端 TEE 浏览器会话 API，不是 agent 自己的 node、worker、tool session、对话状态
- 要操作页面，必须走 `toani sandbox create-session` 和 `toani sandbox execute`

## 先读这一段

如果你是另一个 agent，请先建立下面的心智模型:

1. `toani` 是 CLI，不是 SDK 片段，也不是伪代码。
2. `sandbox` 指 CredBridge 后端提供的远端 TEE 沙盒会话。
3. 不要把 `sandbox` 理解成 OpenClaw、自定义 agent runtime、节点树、browser tab registry、workflow node。
4. 页面操作不是“直接调用浏览器工具”，而是调用:
   `toani sandbox execute <sessionId> --operation-type <type> --params '<json>'`
5. 如果需要页面上下文，先 `create-session`，再 `execute`，必要时 `get-operation` 和 `get-session`，结束时 `terminate`。

## 当前发布版真实能力面

以当前仓库实现为准，公开命令组只有:

- `config`
- `sandbox`
- `--version`
- `--help`

不要假设存在这些命令组，除非你先验证过当前二进制:

- `auth`
- `credentials`
- `tokens`
- `service-accounts`
- `audit`

如果你看到旧文档提到这些能力，优先相信当前 CLI 帮助和 `cli/src/index.ts`。

## 安装

- Registry 安装:

```bash
npm install -g @toani/vault-cli@0.0.5
```

- 本地源码安装:

```bash
cd /Users/yvan/AIWorkspace/credbridge/cli
npm install
npm run build
npm pack
npm install -g ./toani-vault-cli-0.0.5.tgz
```

- 先决条件:

```bash
node -v   # >= 22
npm -v
```

## 全局参数与解析优先级

- 可执行命令: `toani`
- 包名: `@toani/vault-cli`
- 全局参数:
  - `--output json|table`
  - `--base-url <URL>`
  - `--token <TOKEN>`

Base URL 解析优先级:

1. `--base-url`
2. `TOANI_BASE_URL`
3. `CREDBRIDGE_BASE_URL`
4. `config.baseUrl`
5. 默认值 `https://dev-credbridge.bitkinetic.com/`

Token 解析优先级:

1. `--token`
2. `config.token`
3. `TOANI_VAULT_TOKEN`
4. `CREDBRIDGE_TOKEN`

注意:

- 传入 `--base-url`、`--token`、`--output` 会写回 `~/.toani/config.json`
- 自动化默认用 `--output json`
- 人工排障可用 `--output table`

## 标准初始化

```bash
export TOANI_BASE_URL="https://dev-credbridge.bitkinetic.com"
export TOANI_VAULT_TOKEN="<BEARER_TOKEN>"

toani config init --url https://dev-credbridge.bitkinetic.com --token <BEARER_TOKEN>
toani config show
toani --help
```

## Sandbox 命令表

```bash
toani sandbox create-session --service-id <serviceId> --original-intent <intent> [--credential-id <id>] [--start-url <url>]
toani sandbox list-sessions
toani sandbox get-session <sessionId>
toani sandbox terminate <sessionId>
toani sandbox execute <sessionId> --operation-type <type> [--params '<json>']
toani sandbox get-operation <operationId>
toani sandbox stats
```

### `sandbox execute` 支持的 `operation-type`

- `navigate`
- `click`
- `fill`
- `get_text`
- `get_attribute`
- `execute_script`
- `wait`
- `screenshot`
- `export`

### `--params` 常见字段

根据操作类型组合使用:

- `url`
- `selector`
- `value`
- `script`
- `bindings`
- `attribute`

## Agent 必须遵守的执行流程

### 场景 1: 打开页面

```bash
toani sandbox create-session \
  --service-id svc_example \
  --original-intent "Open login page in TEE sandbox" \
  --start-url "https://target-site.com/login"

toani sandbox execute <sessionId> \
  --operation-type navigate \
  --params '{"url":"https://target-site.com/login"}'
```

### 场景 2: 点击按钮

```bash
toani sandbox execute <sessionId> \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'
```

### 场景 3: 输入用户名或密码

```bash
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":"user@example.com"}'
```

如果值来自凭证解析链路，再按后端契约传结构化值，不要自行发明字段。

### 场景 4: 等待元素出现

```bash
toani sandbox execute <sessionId> \
  --operation-type wait \
  --params '{"selector":"#dashboard"}'
```

### 场景 5: 读取文本

```bash
toani sandbox execute <sessionId> \
  --operation-type get_text \
  --params '{"selector":"h1"}'
```

### 场景 6: 取属性

```bash
toani sandbox execute <sessionId> \
  --operation-type get_attribute \
  --params '{"selector":"a.download","attribute":"href"}'
```

### 场景 7: 执行脚本

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return document.title"}'
```

### 场景 8: 截图

```bash
toani sandbox execute <sessionId> \
  --operation-type screenshot \
  --params '{}'
```

### 场景 9: 查询异步操作结果

```bash
toani sandbox get-operation <operationId>
```

### 场景 10: 结束会话

```bash
toani sandbox terminate <sessionId>
```

## 推荐工作流

当 agent 需要“在 TEE 页面里操作”时，按下面顺序执行:

1. `toani config show`
2. `toani sandbox create-session ...`
3. `toani sandbox execute ...`
4. 如果返回 `operationId`，再 `toani sandbox get-operation <operationId>`
5. 需要确认当前会话状态时，执行 `toani sandbox get-session <sessionId>`
6. 完成后执行 `toani sandbox terminate <sessionId>`

不要跳过 `create-session` 直接脑补一个 session。

## 明确禁止的误用

下面这些都不对:

- 把 TEE sandbox 理解成 agent 自己的 nodes / graph / workflow node
- 说“进入 sandbox 节点”而不是调用 `toani sandbox ...`
- 在没有 `sessionId` 的情况下直接执行 `sandbox execute`
- 发明 CLI 没有实现的命令组
- 把 `sandbox` 当成本地 Playwright、浏览器 devtools、OpenClaw 内置网页操作器
- 用自然语言描述代替具体命令和参数

## 最小可执行模板

```bash
toani config init --url https://dev-credbridge.bitkinetic.com --token <BEARER_TOKEN>

toani sandbox create-session \
  --service-id svc_example \
  --original-intent "Investigate target page in TEE sandbox" \
  --start-url "https://target-site.com/login"

toani sandbox execute <sessionId> \
  --operation-type navigate \
  --params '{"url":"https://target-site.com/login"}'

toani sandbox execute <sessionId> \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'

toani sandbox get-session <sessionId>
toani sandbox terminate <sessionId>
```

## 常见错误与修复

- 错误: `Unknown command group`
  - 修复: 先执行 `toani --help`，不要使用旧文档里的未发布命令组

- 错误: `Missing required option --service-id` 或 `--original-intent`
  - 修复: `create-session` 这两个参数必填

- 错误: `Missing required option --operation-type`
  - 修复: `execute` 必须显式传 `--operation-type`

- 错误: `Invalid JSON for --params`
  - 修复: `--params` 必须是合法 JSON 字符串，推荐单引号包裹整段 JSON

- 错误: 401/403
  - 修复: 检查 `--token`、环境变量、`~/.toani/config.json` 的优先级覆盖关系

- 错误: 连到错误环境
  - 修复: 显式传 `--base-url`，再用 `toani config show` 确认

## 安全注意事项

- 本地配置文件路径: `~/.toani/config.json`
- 配置文件可能包含 token，禁止提交到仓库
- 自动化优先用环境变量注入 token
- 不要把 bearer token 写进长期保存的脚本、截图、日志
