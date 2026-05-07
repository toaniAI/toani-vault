# CredBridge 前端架构文档

**生成日期**: 2026-03-25
**最后更新**: 2026-05-07
**框架**: React 19 + TypeScript
**构建工具**: Vite
**样式**: Tailwind CSS + shadcn/ui

---

## 技术栈

| 类别 | 技术                          | 版本         | 用途              |
| ---- | ----------------------------- | ------------ | ----------------- |
| 框架 | React                         | 19.2.0       | UI 框架           |
| 语言 | TypeScript                    | 5.9.3        | 类型安全          |
| 构建 | Vite                          | 7.3.1        | 开发和生产构建    |
| 样式 | Tailwind CSS                  | 3.4.1        | 原子化 CSS        |
| 组件 | shadcn/ui                     | 4.0.5        | UI 组件库         |
| 基础 | Radix UI                      | 1.x          | 无样式组件原语    |
| 状态 | Zustand                       | 5.0.11       | 全局状态管理      |
| 数据 | TanStack Query                | 5.90.21      | 服务器状态管理    |
| 路由 | React Router                  | 7.13.1       | 客户端路由        |
| 字体 | Circular Std + HarmonyOS Sans | local assets | zkme 对齐字体策略 |
| 图标 | Lucide React                  | 0.577.0      | 图标库            |

---

## 主题与设计系统

本轮前端深度重构明确使用了 `/Users/yvan/.codex/skills/zkme-frontend-design/SKILL.md`，并以 `/Users/yvan/AIWorkspace/zkme-web` 作为只读设计源。

### 设计源

- 设计映射文档: `/Users/yvan/AIWorkspace/credbridge/docs/plans/frontend-zkme-source-map.md`
- 核心 token 源: `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/components/ui-design-preview/_base.scss`
- 预览与主题切换参考: `/Users/yvan/AIWorkspace/zkme-web/src/components/ui-design-preview/PreviewContent.vue`
- 字体源: `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/utils/_fonts.scss`
- 代表性页面样式源: `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/views/_Uidesign.scss`

### 当前主题策略

- 默认主题切换为浅底 zkme 风格，主品牌色为 `#005563`
- 保留 `light | dark | system` 兼容结构，但默认落在 `light`
- Tailwind 与 shadcn 颜色别名继续通过 CSS variables 驱动
- 历史深色页面的部分硬编码在第一轮通过兼容层兜底，后续逐页继续清理

### 组件变体策略

- `Button`：主按钮、次按钮、outline、ghost、subtle、destructive 均重绑到 zkme token
- `Card`：默认、muted、elevated、gradient 四类容器气质
- `Badge`：语义徽章以圆角 pill 和轻底色为主
- `Input` / `Dialog` / `Sidebar`：统一采用大圆角、轻边框、浅表面层级

### 静态资源接入

- 品牌资源复制到 `/Users/yvan/AIWorkspace/credbridge/frontend/src/assets/brand/images`
- 字体复制到 `/Users/yvan/AIWorkspace/credbridge/frontend/src/assets/brand/fonts`
- 应用级导航图标继续使用 `lucide-react`
- zkme iconfont 仅保留为参考，不在本轮直接接入 React 应用

---

## 项目结构

```
frontend/src/
├── app/                    # Router、Layout、providers、route wrappers
│   └── pages/            # NotFound 等顶层页面
├── assets/brand/          # zkme / Toani 品牌字体与图片资源
├── components/
│   ├── ui/               # shadcn/ui 组件
│   └── zkme/             # 品牌化页面壳、面板、状态徽章等共享表达
├── features/
│   ├── auth/             # Login / Onboarding / Invitation / Profile 页面
│   ├── credentials/      # 凭证列表、创建弹窗、详情与辅助逻辑
│   ├── audit/            # 审计页面模块（代码保留，当前路由未开放）
│   ├── developer/        # Developer Center 与 API tester
│   ├── tenants/          # tenants/settings/users 页面模块（代码保留，当前路由未开放）
│   └── tokens/           # 受限 token 签发与列表
├── hooks/                 # toast 等共享 hooks
├── lib/                   # `cn()` 等基础工具
├── shared/
│   ├── api/              # Axios client、hooks、types、query client
│   ├── audit/            # 审计展示帮助函数
│   ├── auth/             # session token、logout 等认证桥接逻辑
│   ├── config/           # 运行时配置
│   ├── i18n/             # 国际化消息与 provider
│   ├── lib/              # 共享业务工具
│   └── stores/           # auth / locale / theme Zustand stores
└── index.css             # 全局主题与 token
```

---

## 功能模块 (Features)

### Auth (认证模块)

路径: `features/auth/`

**功能**:

- Privy 登录与后端 session bootstrap
- onboarding 流程
- invitation accept 流程
- profile 页面模块保留在代码中

**组件**:

- `LoginPage`
- `OnboardingPage`
- `InvitationAcceptPage`
- `ProfilePage`

### Credentials (凭证模块)

路径: `features/credentials/`

**功能**:

- 凭证列表、筛选、分页
- 创建凭证弹窗
- 凭证详情侧边栏
- 删除确认、可见性切换、过期时间校验
- `api_key` 场景下的 `provider` / `allowed_domains` / `custom_functions`
- `provider=okx` 时强制要求 `passphrase`
- `allowed_domains` 由逗号/换行文本规范化为 `string[]`
- 当前详情弹窗会拉取 `GET /api/v1/credentials/:id`，但 UI 仍只展示基础元数据

**组件**:

- `CredentialsPage`
- `CredentialsPage.contracts.ts`
- `CredentialsPage.expiration.ts`
- `CredentialsPage.helpers.ts`
- `CredentialsPage.visibility.ts`

### 当前凭证创建交互细节

- `api_key` provider 选项：`okx`、`binance`、`custom`
- `okx` 会显示带校验的 `passphrase`
- `custom` 会显示 `custom_functions` 编辑区
- `allowed_domains` 支持逗号和换行分隔输入
- 详情弹窗当前仅显示：名称、类型、ID、创建时间、过期时间、状态

### Audit (审计模块)

路径: `features/audit/`

**功能**:

- 审计日志列表
- 日志筛选
- 校验结果头部展示
- 当前代码存在，但 `frontend/src/app/router.tsx` 中该路由会重定向到 `/credentials`

**组件**:

- `AuditPage`
- `AuditFilters.ts`
- `AuditVerifyResultHeader.ts`

### Tenants (租户模块)

路径: `features/tenants/`

**功能**:

- tenant 列表与创建流
- users / settings 页面模块
- 当前代码存在，但 `/tenants`、`/settings`、`/users` 路由都会重定向到 `/credentials`

**组件**:

- `TenantsPage`
- `UsersPage`
- `SettingsPage`

### Tokens (受限 Token 模块)

路径: `features/tokens/`

**功能**:

- 基于当前可见 credential IDs 签发受限 token
- token 列表、状态筛选、复制
- token 使用期和 credential 绑定关系展示

**组件**:

- `TokensPage`
- `TokensPage.verify.ts`

### Developer (开发者中心)

路径: `features/developer/`

**功能**:

- 系统概览与文档入口
- API tester
- SDK / CLI / sandbox 示例展示

**组件**:

- `DeveloperCenter`
- `DeveloperCenter.apiTester.ts`

---

## 状态管理

### Zustand Store

```typescript
// shared/stores/authStore.ts
import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";

interface AuthState {
  privyReady: boolean;
  privyAuthenticated: boolean;
  backendSessionReady: boolean;
  isAuthenticated: boolean;
  user: User | null;
  memberships: TenantMembership[];
  currentTenantId: string | null;
  setPrivyState: (ready: boolean, authenticated: boolean) => void;
  setSession: (user: User, memberships: TenantMembership[]) => void;
  switchCurrentTenant: (tenantId: string) => void;
  logout: () => void;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      privyReady: false,
      privyAuthenticated: false,
      backendSessionReady: false,
      isAuthenticated: false,
      user: null,
      memberships: [],
      currentTenantId: null,
      setPrivyState: (ready, authenticated) =>
        set((state) => ({
          privyReady: ready,
          privyAuthenticated: authenticated,
          isAuthenticated: authenticated && state.backendSessionReady,
        })),
      setSession: (user, memberships) =>
        set({
          user,
          memberships,
          currentTenantId: memberships[0]?.tenantId ?? null,
          backendSessionReady: true,
          isAuthenticated: true,
        }),
      switchCurrentTenant: (tenantId) => set({ currentTenantId: tenantId }),
      logout: () =>
        set({
          privyReady: false,
          privyAuthenticated: false,
          backendSessionReady: false,
          isAuthenticated: false,
          user: null,
          memberships: [],
          currentTenantId: null,
        }),
    }),
    {
      name: "auth-storage",
      storage: createJSONStorage(() => localStorage),
    },
  ),
);
```

当前还有两个配套 store：

- `shared/stores/localeStore.ts`：语言状态
- `shared/stores/themeStore.ts`：浅色优先主题切换

### TanStack Query

用于服务器状态管理：

```typescript
// hooks/useCredentials.ts
import { useQuery, useMutation } from "@tanstack/react-query";

export function useCredentials() {
  return useQuery({
    queryKey: ["credentials"],
    queryFn: fetchCredentials,
    staleTime: 5 * 60 * 1000, // 5 分钟
  });
}

export function useCreateCredential() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: createCredential,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["credentials"] });
    },
  });
}
```

---

## API 集成

### API 客户端

```typescript
// shared/api/client.ts
import axios from "axios";
import { readPersistedSessionToken } from "@/shared/auth/sessionTokens";

const apiClient = axios.create({
  baseURL: frontendRuntimeConfig.apiBaseUrl,
  headers: {
    "Content-Type": "application/json",
  },
});

// 请求拦截器 - 添加当前后端 session token
apiClient.interceptors.request.use(async (config) => {
  const token = await readPersistedSessionToken();
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  config.headers["X-Request-ID"] = crypto.randomUUID();
  return config;
});

// 401 时走重新登录流程，而不是刷新本地 token
apiClient.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      prepareForManualRelogin();
    }
    return Promise.reject(error);
  },
);
```

### API Hooks

```typescript
// features/credentials/api.ts
export async function fetchCredentials(): Promise<Credential[]> {
  const response = await apiClient.get("/credentials");
  return response.data;
}

export async function createCredential(
  data: CreateCredentialRequest,
): Promise<Credential> {
  const response = await apiClient.post("/credentials", data);
  return response.data;
}

export async function decryptCredential(id: string): Promise<DecryptResponse> {
  const response = await apiClient.post(`/credentials/${id}/decrypt`);
  return response.data;
}
```

---

## 组件设计

### shadcn/ui 组件

使用的组件库组件：

| 组件           | 用途       |
| -------------- | ---------- |
| `Button`       | 按钮       |
| `Card`         | 卡片容器   |
| `Dialog`       | 弹窗对话框 |
| `Form`         | 表单处理   |
| `Input`        | 文本输入   |
| `Select`       | 下拉选择   |
| `Table`        | 数据表格   |
| `Tabs`         | 标签页     |
| `Toast`        | 消息提示   |
| `DropdownMenu` | 下拉菜单   |
| `Tooltip`      | 工具提示   |
| `Progress`     | 进度条     |
| `ScrollArea`   | 滚动区域   |
| `Separator`    | 分隔线     |
| `Avatar`       | 头像       |
| `Badge`        | 徽章       |
| `AlertDialog`  | 确认对话框 |
| `Accordion`    | 手风琴     |
| `Collapsible`  | 可折叠     |
| `Label`        | 标签       |

### 组件示例

```tsx
// features/credentials/components/CredentialCard.tsx
import { Card, CardHeader, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

interface CredentialCardProps {
  credential: Credential;
  onDecrypt: (id: string) => void;
}

export function CredentialCard({ credential, onDecrypt }: CredentialCardProps) {
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <h3 className="font-semibold">{credential.service_id}</h3>
          <Badge variant={credential.is_deleted ? "destructive" : "default"}>
            {credential.credential_type}
          </Badge>
        </div>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground">
          用户: {credential.user_id_hash}
        </p>
        <p className="text-sm text-muted-foreground">
          创建: {new Date(credential.created_at).toLocaleDateString()}
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={() => onDecrypt(credential.credential_id)}
        >
          解密查看
        </Button>
      </CardContent>
    </Card>
  );
}
```

---

## 路由配置

```typescript
// app/router.tsx
import { createBrowserRouter, Navigate } from 'react-router-dom';

export const router = createBrowserRouter([
  { path: '/invitation/accept', element: <InvitationAcceptPage /> },
  { path: '/login', element: <PublicRoute><LoginPage /></PublicRoute> },
  {
    path: '/',
    element: <ProtectedRoute><MainLayout /></ProtectedRoute>,
    children: [
      { index: true, element: <Navigate to="/credentials" replace /> },
      { path: 'credentials', element: <CredentialsPage /> },
      { path: 'tokens', element: <TokensPage /> },
      { path: 'developer', element: <DeveloperCenter /> },
      { path: 'audit', element: <Navigate to="/credentials" replace /> },
      { path: 'tenants', element: <Navigate to="/credentials" replace /> },
      { path: 'settings', element: <Navigate to="/credentials" replace /> },
      { path: 'users', element: <Navigate to="/credentials" replace /> },
      { path: 'profile', element: <Navigate to="/credentials" replace /> },
    ],
  },
  { path: '/onboarding', element: <ProtectedRoute><OnboardingPage /></ProtectedRoute> },
  { path: '*', element: <NotFoundPage /> },
]);
```

说明：

- 当前公开控制台不是 dashboard-first，而是 credentials-first。
- `audit` / `tenants` 相关页面源码仍在仓库中，但默认路由已经收口到 `/credentials`。

---

## 样式系统

### Tailwind 配置

```javascript
// tailwind.config.js
module.exports = {
  darkMode: ["class"],
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        // ... 更多颜色
      },
      fontFamily: {
        sans: ["Geist Variable", "system-ui", "sans-serif"],
      },
    },
  },
};
```

### CSS 变量

```css
/* index.css */
@tailwind base;
@tailwind components;
@tailwind utilities;

@layer base {
  :root {
    --background: 0 0% 100%;
    --foreground: 222.2 84% 4.9%;
    --card: 0 0% 100%;
    --card-foreground: 222.2 84% 4.9%;
    --popover: 0 0% 100%;
    --popover-foreground: 222.2 84% 4.9%;
    --primary: 222.2 47.4% 11.2%;
    --primary-foreground: 210 40% 98%;
    --secondary: 210 40% 96.1%;
    --secondary-foreground: 222.2 47.4% 11.2%;
    --muted: 210 40% 96.1%;
    --muted-foreground: 215.4 16.3% 46.9%;
    --accent: 210 40% 96.1%;
    --accent-foreground: 222.2 47.4% 11.2%;
    --destructive: 0 84.2% 60.2%;
    --destructive-foreground: 210 40% 98%;
    --border: 214.3 31.8% 91.4%;
    --input: 214.3 31.8% 91.4%;
    --ring: 222.2 84% 4.9%;
    --radius: 0.5rem;
  }

  .dark {
    --background: 222.2 84% 4.9%;
    --foreground: 210 40% 98%;
    /* ... 暗色模式变量 */
  }
}
```

---

## 开发规范

### 文件命名

- 组件: `PascalCase.tsx`
- Hooks: `camelCase.ts` (以 `use` 开头)
- 工具: `camelCase.ts`
- 类型: `PascalCase.ts` 或 `types.ts`

### 组件结构

```tsx
// 1. Imports
import { useState } from "react";

// 2. Types
interface Props {
  title: string;
}

// 3. Component
export function MyComponent({ title }: Props) {
  // State
  const [count, setCount] = useState(0);

  // Handlers
  const handleClick = () => setCount((c) => c + 1);

  // Render
  return (
    <div>
      <h1>{title}</h1>
      <button onClick={handleClick}>Count: {count}</button>
    </div>
  );
}
```

### 性能优化

1. **使用 React.memo 优化子组件**
2. **使用 useCallback 缓存事件处理函数**
3. **使用 useMemo 缓存计算结果**
4. **使用 TanStack Query 缓存服务器状态**
5. **路由懒加载**

---

## 构建和部署

### 开发

```bash
cd frontend
npm install
npm run dev
```

### 构建

```bash
npm run build
```

### 代码检查

```bash
npm run lint
npm run lint:fix
npm run format:check
npm run format
```

---

_本文档由 BMAD document-project 工作流自动生成_
