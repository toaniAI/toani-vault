# CredBridge Web 前端 - 产品需求文档 (PRD)

---

## 1. 文档信息

| 属性 | 值 |
|-----|-----|
| 产品名称 | CredBridge Web Console |
| 版本 | MVP 1.0 |
| 状态 | Draft |
| 创建日期 | 2026-03-11 |
| 负责人 | claude_kimi |
| 关联文档 | [产品简报](./frontend-brief.md) |

---

## 2. 产品概述

### 2.1 产品愿景

CredBridge Web Console 是 CredBridge TEE 凭证保险库的可视化管理界面，让用户能够安全、高效地管理敏感凭证、API Token 和访问权限，同时提供完整的审计追踪能力。

### 2.2 目标用户

- **系统管理员**: 管理租户配置、用户权限、系统设置
- **安全管理员**: 监控审计日志、管理安全策略、合规检查
- **开发人员**: 管理 API Token、查看凭证、调试集成
- **终端用户**: 管理个人凭证、查看访问历史

### 2.3 用户故事地图

```
作为 [角色]
我想要 [功能]
以便 [价值]

├── 认证与授权
│   ├── 作为用户，我想要安全登录，以便访问我的凭证
│   ├── 作为用户，我想要 MFA 支持，以便增强账户安全
│   └── 作为管理员，我想要管理用户权限，以便控制访问
│
├── 凭证管理
│   ├── 作为用户，我想要创建凭证，以便存储敏感数据
│   ├── 作为用户，我想要编辑凭证，以便更新信息
│   ├── 作为用户，我想要删除凭证，以便清理不需要的数据
│   ├── 作为用户，我想要查看凭证列表，以便快速浏览
│   └── 作为用户，我想要搜索凭证，以便快速定位
│
├── Token 管理
│   ├── 作为开发者，我想要生成 API Token，以便程序访问
│   ├── 作为开发者，我想要查看 Token 使用记录，以便监控
│   └── 作为开发者，我想要撤销 Token，以便控制访问
│
├── 审计与日志
│   ├── 作为安全员，我想要查看操作日志，以便审计
│   ├── 作为安全员，我想要过滤日志，以便定位问题
│   └── 作为管理员，我想要导出日志，以便合规报告
│
└── 租户管理
    ├── 作为管理员，我想要配置租户设置，以便定制系统
    ├── 作为管理员，我想要管理用户，以便团队协作
    └── 作为管理员，我想要查看配额使用，以便资源规划
```

---

## 3. 功能需求

### 3.1 模块: 用户认证 (AUTH)

#### AUTH-001: 用户登录

**需求描述**: 用户通过用户名/密码登录系统

**验收标准**:
- [ ] 支持用户名/密码登录表单
- [ ] 密码输入框支持显示/隐藏切换
- [ ] 登录失败时显示友好错误信息（不暴露用户是否存在）
- [ ] 支持 "记住我" 选项
- [ ] 登录成功后重定向到仪表板
- [ ] 登录页支持深色/浅色主题

**技术备注**:
- 使用后端 `/auth/login` API
- Token 存储在 HttpOnly Cookie 或 secure localStorage
- 实现自动 token 刷新

#### AUTH-002: 多因素认证 (MFA)

**需求描述**: 支持 TOTP 基于时间的一次性密码

**验收标准**:
- [ ] 支持 MFA 启用/禁用
- [ ] 启用 MFA 时显示 QR Code 供扫描
- [ ] 提供备用恢复码
- [ ] 登录时如启用 MFA，要求输入验证码
- [ ] 支持 "信任此设备" 选项

**技术备注**:
- 使用后端 `/auth/mfa` 相关 API
- 使用 `qrcode.react` 生成 QR Code
- 使用 `otplib` 验证 TOTP

#### AUTH-003: 会话管理

**需求描述**: 用户可以查看和管理活跃会话

**验收标准**:
- [ ] 显示当前活跃会话列表
- [ ] 显示会话设备、IP、位置、时间信息
- [ ] 支持终止其他会话
- [ ] 支持终止所有其他会话

#### AUTH-004: 登出

**验收标准**:
- [ ] 支持用户主动登出
- [ ] 登出后清除本地 token
- [ ] 登出后重定向到登录页

---

### 3.2 模块: 凭证管理 (CRED)

#### CRED-001: 凭证列表

**需求描述**: 显示用户有权访问的所有凭证

**验收标准**:
- [ ] 表格/卡片双视图切换
- [ ] 显示凭证名称、类型、更新时间、状态
- [ ] 支持分页（每页 10/25/50 条）
- [ ] 支持排序（名称、更新时间）
- [ ] 空状态时显示引导创建
- [ ] 支持键盘快捷键（Ctrl/Cmd + K 搜索）

**技术备注**:
- 使用后端 `/credentials` API
- 实现虚拟滚动处理大数据量

#### CRED-002: 创建凭证

**需求描述**: 创建新的凭证条目

**验收标准**:
- [ ] 支持多种凭证类型（密码、API Key、证书、文本）
- [ ] 表单验证（必填项、长度限制）
- [ ] 密码生成器（可配置长度、字符集）
- [ ] 标签/分类选择
- [ ] 备注/描述输入
- [ ] 创建成功后显示确认

**凭证类型字段**:

| 类型 | 字段 |
|-----|-----|
| Password | username, password, url |
| API Key | key_name, api_key, base_url |
| Certificate | cert_name, certificate, private_key |
| Secure Note | title, content |

#### CRED-003: 编辑凭证

**验收标准**:
- [ ] 加载现有凭证数据
- [ ] 支持修改所有字段
- [ ] 显示最后修改时间
- [ ] 保存前确认提示（如有未保存更改）

#### CRED-004: 删除凭证

**验收标准**:
- [ ] 删除前二次确认
- [ ] 支持软删除（可恢复）
- [ ] 删除后显示成功提示
- [ ] 列表自动刷新

#### CRED-005: 查看凭证详情

**验收标准**:
- [ ] 在抽屉/弹窗中显示完整信息
- [ ] 敏感字段默认隐藏（点击显示）
- [ ] 支持一键复制字段值
- [ ] 显示访问历史（最近 5 次）
- [ ] 显示审计日志链接

#### CRED-006: 搜索与过滤

**验收标准**:
- [ ] 全局搜索（名称、标签、描述）
- [ ] 按类型过滤
- [ ] 按标签过滤
- [ ] 按日期范围过滤
- [ ] 搜索结果高亮

---

### 3.3 模块: Token 管理 (TOKEN)

#### TOKEN-001: Token 列表

**需求描述**: 显示用户创建的所有 API Token

**验收标准**:
- [ ] 列表显示 Token 名称、权限范围、创建时间、过期时间、状态
- [ ] 显示 Token 最后使用时间
- [ ] 支持排序和分页

#### TOKEN-002: 生成 Token

**验收标准**:
- [ ] 支持自定义 Token 名称
- [ ] 选择权限范围（只读、读写、管理员）
- [ ] 设置过期时间（7天、30天、90天、永不过期）
- [ ] 生成后仅显示一次完整 Token
- [ ] 提供复制按钮
- [ ] 显示安全警告（Token 不会再次显示）

#### TOKEN-003: 撤销 Token

**验收标准**:
- [ ] 支持撤销单个 Token
- [ ] 撤销前确认提示
- [ ] 已撤销 Token 标记为红色/失效状态

#### TOKEN-004: Token 使用统计

**验收标准**:
- [ ] 显示 Token 调用次数
- [ ] 显示最近使用时间
- [ ] 显示使用 IP 列表

---

### 3.4 模块: 审计日志 (AUDIT)

#### AUDIT-001: 日志列表

**需求描述**: 显示系统操作审计日志

**验收标准**:
- [ ] 表格显示时间、用户、操作、资源、结果、IP
- [ ] 支持分页（每页 50/100/200 条）
- [ ] 支持按时间倒序排列
- [ ] 支持点击展开查看详情

#### AUDIT-002: 日志过滤

**验收标准**:
- [ ] 按用户过滤
- [ ] 按操作类型过滤（CREATE/READ/UPDATE/DELETE）
- [ ] 按资源类型过滤
- [ ] 按时间范围过滤（今天、本周、本月、自定义）
- [ ] 按结果状态过滤（成功/失败）

#### AUDIT-003: 日志导出

**验收标准**:
- [ ] 支持导出为 CSV
- [ ] 支持导出为 JSON
- [ ] 导出范围可选择（当前筛选结果/全部）

---

### 3.5 模块: 租户管理 (TENANT)

#### TENANT-001: 租户设置

**需求描述**: 配置租户级设置

**验收标准**:
- [ ] 显示租户名称、ID、创建时间
- [ ] 修改租户显示名称
- [ ] 配置会话超时时间
- [ ] 配置密码策略（最小长度、复杂度）
- [ ] 启用/禁用功能开关

#### TENANT-002: 用户管理

**需求描述**: 管理租户内用户

**验收标准**:
- [ ] 用户列表（用户名、角色、状态、最后登录）
- [ ] 邀请新用户
- [ ] 修改用户角色
- [ ] 启用/禁用用户
- [ ] 重置用户密码

#### TENANT-003: 配额管理

**验收标准**:
- [ ] 显示当前配额使用情况
- [ ] 凭证数量配额
- [ ] Token 数量配额
- [ ] 存储空间配额
- [ ] API 调用配额

---

## 4. 非功能需求

### 4.1 性能需求

| 指标 | 要求 | 测量方法 |
|-----|------|---------|
| 首屏加载 | < 3s (FCP) | Lighthouse |
| 可交互时间 | < 5s (TTI) | Lighthouse |
| 首字节时间 | < 200ms | Web Vitals |
| 累积布局偏移 | < 0.1 (CLS) | Web Vitals |
| 交互响应 | < 100ms | Chrome DevTools |
| 列表渲染 | < 500ms (1000条) | 手动测试 |

### 4.2 安全需求

| 需求 | 描述 |
|-----|------|
| XSS 防护 | 所有用户输入转义，使用 DOMPurify |
| CSRF 防护 | 使用 SameSite Cookie + CSRF Token |
| CSP | 配置内容安全策略 |
| HSTS | 强制 HTTPS |
| 敏感信息 | 密码/Token 输入框使用 type="password" |
| 自动登出 | 空闲 30 分钟后自动登出 |
| 会话安全 | Token 存储在 HttpOnly Cookie |

### 4.3 可访问性需求

| 需求 | 描述 |
|-----|------|
| WCAG 标准 | 2.1 Level AA |
| 键盘导航 | 所有功能支持键盘操作 |
| 屏幕阅读器 | 支持 ARIA 标签 |
| 颜色对比度 | 最小 4.5:1 |
| 焦点指示 | 清晰的焦点样式 |
| 文本缩放 | 支持 200% 缩放 |

### 4.4 兼容性需求

| 浏览器 | 版本要求 |
|-------|---------|
| Chrome | 最新 2 个版本 |
| Firefox | 最新 2 个版本 |
| Safari | 最新 2 个版本 |
| Edge | 最新 2 个版本 |

---

## 5. 界面设计

### 5.1 整体布局

```
┌─────────────────────────────────────────────────────────┐
│  Logo    Search Bar                    User Menu   🔔   │  Header (64px)
├──────────┬──────────────────────────────────────────────┤
│          │                                              │
│  Dashboard│              Main Content Area               │
│  ────────│                                              │
│  Credentials                                         │
│  Tokens                                                │  Sidebar (240px)
│  Audit Logs                                            │
│  ────────│                                              │
│  Settings                                              │
│  Users                                                 │
│          │                                              │
└──────────┴──────────────────────────────────────────────┘
```

### 5.2 页面清单

| 页面 | 路径 | 描述 |
|-----|------|------|
| 登录 | `/login` | 用户登录 |
| 仪表板 | `/dashboard` | 概览信息 |
| 凭证列表 | `/credentials` | 凭证管理主页 |
| 凭证详情 | `/credentials/:id` | 单个凭证详情 |
| Token 管理 | `/tokens` | API Token 管理 |
| 审计日志 | `/audit` | 操作日志 |
| 租户设置 | `/settings` | 系统配置 |
| 用户管理 | `/users` | 用户管理 |
| 个人资料 | `/profile` | 个人信息 |

### 5.3 主题规范

**深色主题**:
- 背景: `#0f0f0f`
- 表面: `#1a1a1a`
- 边框: `#2a2a2a`
- 主文本: `#ffffff`
- 次文本: `#a0a0a0`

**浅色主题**:
- 背景: `#f5f5f5`
- 表面: `#ffffff`
- 边框: `#e0e0e0`
- 主文本: `#1a1a1a`
- 次文本: `#666666`

---

## 6. API 集成

### 6.1 认证 API

| 端点 | 方法 | 描述 |
|-----|------|------|
| `/auth/login` | POST | 用户登录 |
| `/auth/logout` | POST | 用户登出 |
| `/auth/refresh` | POST | 刷新 Token |
| `/auth/mfa/setup` | POST | 设置 MFA |
| `/auth/mfa/verify` | POST | 验证 MFA |

### 6.2 凭证 API

| 端点 | 方法 | 描述 |
|-----|------|------|
| `/credentials` | GET | 列表 |
| `/credentials` | POST | 创建 |
| `/credentials/:id` | GET | 详情 |
| `/credentials/:id` | PUT | 更新 |
| `/credentials/:id` | DELETE | 删除 |

### 6.3 Token API

| 端点 | 方法 | 描述 |
|-----|------|------|
| `/tokens` | GET | 列表 |
| `/tokens` | POST | 创建 |
| `/tokens/:id` | DELETE | 撤销 |

### 6.4 审计 API

| 端点 | 方法 | 描述 |
|-----|------|------|
| `/audit/logs` | GET | 日志列表 |
| `/audit/logs/export` | POST | 导出日志 |

---

## 7. 数据结构

### 7.1 Credential

```typescript
interface Credential {
  id: string;           // UUID v7
  name: string;
  type: 'password' | 'api_key' | 'certificate' | 'note';
  data: CredentialData;
  tags: string[];
  description?: string;
  created_at: string;
  updated_at: string;
  created_by: string;
  version: number;
}
```

### 7.2 Token

```typescript
interface Token {
  id: string;
  name: string;
  scopes: string[];
  expires_at?: string;
  last_used_at?: string;
  created_at: string;
  status: 'active' | 'revoked' | 'expired';
}
```

### 7.3 AuditLog

```typescript
interface AuditLog {
  id: string;
  timestamp: string;
  user_id: string;
  username: string;
  action: 'CREATE' | 'READ' | 'UPDATE' | 'DELETE';
  resource_type: string;
  resource_id: string;
  result: 'success' | 'failure';
  ip_address: string;
  user_agent?: string;
  details?: Record<string, any>;
}
```

---

## 8. 错误处理

### 8.1 错误码映射

| HTTP 状态 | 业务错误码 | 用户提示 |
|----------|-----------|---------|
| 400 | INVALID_REQUEST | 请求参数错误 |
| 401 | UNAUTHORIZED | 登录已过期，请重新登录 |
| 403 | FORBIDDEN | 权限不足 |
| 404 | NOT_FOUND | 资源不存在 |
| 409 | CONFLICT | 资源已存在 |
| 422 | VALIDATION_ERROR | 数据验证失败 |
| 429 | RATE_LIMITED | 请求过于频繁，请稍后再试 |
| 500 | INTERNAL_ERROR | 服务器错误，请稍后重试 |

### 8.2 错误展示

- 表单错误: 内联显示在字段下方
- 全局错误: Toast 通知
- 网络错误: 自动重试 3 次，后显示友好提示

---

## 9. 待决策事项

| 事项 | 选项 | 建议 |
|-----|------|------|
| UI 框架 | Ant Design / shadcn/ui | shadcn/ui + Tailwind |
| 状态管理 | Zustand / Redux / Jotai | Zustand |
| 数据获取 | TanStack Query / SWR | TanStack Query v5 |
| 路由 | React Router / TanStack Router | React Router v6 |
| 构建工具 | Vite / Next.js | Vite |
| 测试 | Vitest / Jest | Vitest |

---

## 10. 附录

### 10.1 变更日志

| 版本 | 日期 | 变更内容 | 作者 |
|-----|------|---------|------|
| 0.1 | 2026-03-11 | 初始版本 | claude_kimi |

### 10.2 参考文档

- [产品简报](./frontend-brief.md)
- [API 文档](../API.md)
- [CredBridge README](../README.md)

---

**文档状态**: Draft
**下次评审**: 架构设计完成后
