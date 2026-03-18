# CredBridge 问题还原报告 (BMAD Phase 2)

**还原人**: claude_glm (后端开发专家)
**还原日期**: 2026-03-12
**还原版本**: v1.0

---

## 1. 环境状态

### 1.1 后端服务

**检查命令**:
```bash
curl http://localhost:8082/health
curl http://localhost:8082/health/detail
ps aux | grep vault-ser | grep -v grep
```

**检查结果**:
```json
// /health
{"status":"healthy","version":"0.1.0","timestamp":1773280749}

// /health/detail
{"status":"healthy","version":"0.1.0","timestamp":1773280749,"components":{"vault":"healthy","enclave":"simulation_mode","audit_log":"healthy"}}
```

**进程状态**:
```
yvan  91614  0.0  0.1 435330912  9632  ??  SN  9:37上午  0:00.01 ./target/release/vault-service
```

**结论**: ✅ 后端服务运行正常，健康检查通过

### 1.2 前端服务

**检查命令**:
```bash
lsof -i :5173 -P
lsof -i :3000 -P
```

**检查结果**: 两个端口均无服务运行

**结论**: ❌ 前端服务未运行（Phase 1 分析正确）

---

## 2. P0 问题还原：Token 生成缺失

### 2.1 登录 API 测试

**命令**:
```bash
curl -s -X POST http://localhost:8082/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "admin123"}'
```

**响应**:
```json
{
  "access_token": "v4.local.oKkieLuihPMmBvJw__6krEwz01RyPXTku23sXNTt_m34Bktpw8byAs0OSAvgQwXEK0gyvj1nvB1WVakYOs03g997oEX-F70DSCZFAT4_xRy3Te4NJu4ApUB66cIR41mHxBX3FoDWZj5dOMWLFpZjRxFr8JxKXyVOh4oGtcKgeYJ-cYQG6zeT6_jUUaUnqP0bKeeipBnuZPVRZlQulARbB7a6qXxJZk4UkEArHasT4fEjIvvBQdi01iz6MBeOWFSfMSLfBRrF_FhWmDyfFgI9dVOZioeMBjz-SFb4RvXKUDn4p1bL3OKivvRfBFsiyH0OdRPpC4mFF-wwsnUPnfDgQ0NdfWA2OZ458yv4UQb5PB7pvqVNCMbyJu8L-uK2",
  "refresh_token": "rt_tenant-001_user-001_019cdfc5-1a3e-7f62-bc7b-722682a30115",
  "token_type": "Bearer",
  "expires_in": 900,
  "user_id": "user-001",
  "tenant_id": "tenant-001",
  "scope": "admin"
}
```

**结果**: ✅ 登录 API 正常返回 Token

### 2.2 凭证 API 测试（无 Token）

**命令**:
```bash
curl -s -X GET http://localhost:8082/api/v1/credentials
```

**响应**:
```
Missing request extension: Extension of type `vault_service::api::middleware::ValidatedToken` was not found. Perhaps you forgot to add it? See `axum::Extension`.
```

**结果**: ✅ 确认错误，暴露了内部 Rust 类型名

### 2.3 凭证 API 测试（无效 Token）

**命令**:
```bash
curl -s -X GET http://localhost:8082/api/v1/credentials \
  -H "Authorization: Bearer invalid_token"
```

**响应**:
```
Missing request extension: Extension of type `vault_service::api::middleware::ValidatedToken` was not found. Perhaps you forgot to add it? See `axum::Extension`.
```

**结果**: ✅ 确认错误，即使提供 Token 也失败（中间件未配置）

### 2.4 凭证 API 测试（有效 Token）

**命令**:
```bash
curl -s -X GET http://localhost:8082/api/v1/credentials \
  -H "Authorization: Bearer <登录返回的 Token>"
```

**响应**:
```
Missing request extension: Extension of type `vault_service::api::middleware::ValidatedToken` was not found. Perhaps you forgot to add it? See `axum::Extension`.
```

**结果**: ✅ 确认问题：即使使用有效 Token，由于认证中间件未配置到路由，请求仍然失败

### 2.5 问题确认

| 检查项 | 结果 |
|--------|------|
| 登录 API 是否返回 Token？ | ✅ 是 |
| 凭证 API 是否返回 Extension 错误？ | ✅ 是 |
| 错误信息是否包含 Rust 类型名？ | ✅ 是 |
| 使用有效 Token 是否仍失败？ | ✅ 是（中间件未配置） |

**P0 问题确认**: ✅ 问题完全复现，根本原因是认证中间件未配置到受保护的路由

---

## 3. P1 问题还原：错误格式不统一

### 3.1 凭证 API 错误响应

**命令**:
```bash
curl -s -X POST http://localhost:8082/api/v1/credentials \
  -H "Content-Type: application/json" \
  -d '{"service_id": "test"}'
```

**响应**:
```
Missing request extension: Extension of type `vault_service::api::middleware::ValidatedToken` was not found. Perhaps you forgot to add it? See `axum::Extension`.
```

**格式**: 纯文本（Axum 框架内部错误）

### 3.2 审计 API 错误响应

**命令**:
```bash
curl -s -X GET http://localhost:8082/api/v1/audit/logs
```

**响应**:
```json
{
  "success": false,
  "data": {
    "items": [],
    "total": 0,
    "page": 1,
    "page_size": 20,
    "total_pages": 0
  },
  "error": "未提供有效的 Token"
}
```

**格式**: JSON（应用自定义格式）

### 3.3 问题确认

| 检查项 | 结果 |
|--------|------|
| 凭证 API 返回纯文本错误？ | ✅ 是 |
| 审计 API 返回 JSON 错误？ | ✅ 是 |
| 格式是否不一致？ | ✅ 是 |
| 错误是否暴露内部实现？ | ✅ 是（Rust 类型名） |

**P1 问题确认**: ✅ 问题完全复现，两个 API 返回错误格式不一致

---

## 4. P2 问题还原：前端未运行

### 4.1 前端状态检查

**命令**:
```bash
lsof -i :5173 -P
lsof -i :3000 -P
ls -la /Users/yvan/AIWorkspace/credbridge/frontend/
ls /Users/yvan/AIWorkspace/credbridge/frontend/node_modules
```

**检查结果**:
- 端口 5173 和 3000 均无服务
- 前端目录存在，结构完整
- node_modules 已安装，依赖齐全
- 存在 dist 目录（已构建过）

**结论**: ❌ 前端未运行，但环境已就绪

### 4.2 前端启动测试

**命令**:
```bash
cd /Users/yvan/AIWorkspace/credbridge/frontend && npm run dev &
```

**输出**:
```
> frontend@0.0.0 dev
> vite

Port 5173 is in use, trying another one...

  VITE v7.3.1  ready in 84 ms

  ➯  Local:   http://localhost:5174/
```

**结果**: 前端启动成功，运行在端口 5174（5173 被占用）

### 4.3 前端访问测试

**命令**:
```bash
curl -s --noproxy localhost http://localhost:5174 | head -20
```

**响应**:
```html
<!doctype html>
<html lang="en">
  <head>
    <script type="module">import { injectIntoGlobalHook } from "/@react-refresh";
injectIntoGlobalHook(window);
window.$RefreshReg$ = () => {};
window.$RefreshSig$ = () => (type) => type;</script>

    <script type="module" src="/@vite/client"></script>

    <meta charset="UTF-8" />
    <link rel="icon" type="image/svg+xml" href="/vite.svg" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>frontend</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

**结果**: ✅ 前端页面可正常访问

### 4.4 问题确认

| 检查项 | 结果 |
|--------|------|
| 前端是否未运行？ | ✅ 是（初始状态） |
| 启动后是否可以访问？ | ✅ 是 |
| 依赖是否已安装？ | ✅ 是 |
| 是否有构建产物？ | ✅ 是（dist 目录） |

**P2 问题确认**: ✅ 问题确认，前端服务未启动，但环境已就绪

---

## 5. 解决方案预验证

### 5.1 P0 修复建议验证

**Phase 1 建议**: 在 `src/main.rs` 的 `build_api_routes` 函数中添加认证中间件

**验证结果**:
- ✅ 登录 API 已可生成有效 Token
- ✅ 认证中间件代码已存在（`src/api/middleware.rs`）
- ❌ 中间件未配置到凭证/审计路由

**修复方向确认**:
1. 在 `build_api_routes` 中创建 `TokenStore` 和 `secret_key`
2. 使用 `axum::middleware::from_fn_with_state` 应用中间件
3. 从环境变量读取 `TOKEN_SECRET_KEY` 确保持久化

### 5.2 P1 修复建议验证

**Phase 1 建议**: 添加全局错误处理中间件，统一错误响应格式

**验证结果**:
- ✅ 审计 API 已有 JSON 错误格式（可作为参考）
- ❌ 凭证 API 暴露框架内部错误
- ❌ 无全局错误处理机制

**修复方向确认**:
1. 定义统一的 `ErrorResponse` 类型
2. 添加全局错误处理中间件捕获框架错误
3. 错误信息中移除 Rust 类型名等技术细节

### 5.3 P2 修复建议验证

**Phase 1 建议**: 启动前端开发服务器

**验证结果**:
- ✅ 依赖已安装，无需额外安装
- ✅ 启动命令 `npm run dev` 正常工作
- ✅ 页面可正常访问

**修复方向确认**:
1. 短期：手动启动前端 `npm run dev`
2. 长期：更新部署文档，添加前端启动步骤
3. 可选：配置后端服务静态文件（生产模式）

---

## 6. 新发现

### 6.1 端口冲突

启动前端时发现端口 5173 已被占用，Vite 自动切换到 5174。

**现有进程**:
```
node /Users/yvan/AIWorkspace/credbridge/frontend/node_modules/.bin/vite --port 5175
```

**建议**: 清理旧的前端进程或统一端口配置

### 6.2 代理配置影响

访问 localhost 时返回 502 Bad Gateway，需要使用 `--noproxy` 参数。

**影响**: 可能影响前端连接后端 API

**建议**: 检查 `.env` 或 Vite 配置中的代理设置

---

## 7. 结论

### 7.1 问题还原总结

| 问题 | Phase 1 分析 | Phase 2 还原 | 一致性 |
|------|--------------|--------------|--------|
| P0: Token 缺失 | 认证中间件未配置 | ✅ 完全复现 | 100% |
| P1: 错误格式 | 格式不统一 | ✅ 完全复现 | 100% |
| P2: 前端未运行 | 未启动服务 | ✅ 完全复现 | 100% |

### 7.2 Phase 3 准备

**已验证可进入 Phase 3 修复实施**:

1. **P0 优先级最高** - 需要修改 `src/main.rs`，添加认证中间件
2. **P1 次优先级** - 需要添加全局错误处理
3. **P2 可独立处理** - 启动前端服务

### 7.3 修复建议

**推荐修复顺序**:
1. 修复 P0（认证中间件）- 预计 2-3 小时
2. 修复 P1（错误格式）- 预计 1-2 小时
3. 处理 P2（前端启动）- 预计 10 分钟

---

**报告结束**

生成时间：2026-03-12
版本：v1.0
还原人：claude_glm (后端开发专家)