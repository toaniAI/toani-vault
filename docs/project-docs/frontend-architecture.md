# CredBridge 前端架构文档

**生成日期**: 2026-03-18
**框架**: React 19 + TypeScript
**构建工具**: Vite
**样式**: Tailwind CSS + shadcn/ui

---

## 技术栈

| 类别 | 技术 | 版本 | 用途 |
|------|------|------|------|
| 框架 | React | 19.2.0 | UI 框架 |
| 语言 | TypeScript | 5.9.3 | 类型安全 |
| 构建 | Vite | 7.3.1 | 开发和生产构建 |
| 样式 | Tailwind CSS | 3.4.1 | 原子化 CSS |
| 组件 | shadcn/ui | 4.0.5 | UI 组件库 |
| 基础 | Radix UI | 1.x | 无样式组件原语 |
| 状态 | Zustand | 5.0.11 | 全局状态管理 |
| 数据 | TanStack Query | 5.90.21 | 服务器状态管理 |
| 路由 | React Router | 7.13.1 | 客户端路由 |
| 字体 | Geist | 5.2.8 | 字体家族 |
| 图标 | Lucide React | 0.577.0 | 图标库 |

---

## 项目结构

```
frontend/src/
├── app/                    # 应用入口
│   └── main.tsx           # 主入口文件
├── components/            # 共享 UI 组件
│   └── ui/               # shadcn/ui 组件
├── features/              # 功能模块
│   ├── auth/             # 认证功能
│   ├── credentials/      # 凭证管理
│   ├── audit/            # 审计日志
│   ├── dashboard/        # 仪表板
│   ├── tenants/          # 多租户管理
│   ├── tokens/           # Token 管理
│   ├── layout/           # 布局组件
│   └── developer/        # 开发者工具
├── hooks/                 # 自定义 Hooks
├── lib/                   # 工具函数
├── shared/                # 共享资源
│   ├── api/              # API 客户端
│   ├── types/            # TypeScript 类型
│   └── utils/            # 工具函数
└── index.css             # 全局样式
```

---

## 功能模块 (Features)

### Auth (认证模块)

路径: `features/auth/`

**功能**:
- 登录/登出
- Token 管理
- 权限检查

**组件**:
- `LoginForm` - 登录表单
- `AuthGuard` - 认证守卫
- `TokenManager` - Token 管理界面

### Credentials (凭证模块)

路径: `features/credentials/`

**功能**:
- 凭证列表
- 创建/编辑凭证
- 凭证解密
- 版本历史

**组件**:
- `CredentialList` - 凭证列表
- `CredentialForm` - 凭证表单
- `CredentialDetail` - 凭证详情
- `DecryptModal` - 解密弹窗
- `VersionHistory` - 版本历史

### Audit (审计模块)

路径: `features/audit/`

**功能**:
- 审计日志列表
- 日志筛选
- 日志导出

**组件**:
- `AuditLogList` - 审计日志列表
- `AuditLogFilter` - 日志筛选器
- `AuditLogDetail` - 日志详情

### Dashboard (仪表板)

路径: `features/dashboard/`

**功能**:
- 统计数据展示
- 活动图表
- 快捷操作

**组件**:
- `StatsCards` - 统计卡片
- `ActivityChart` - 活动图表
- `QuickActions` - 快捷操作

### Tenants (租户模块)

路径: `features/tenants/`

**功能**:
- 租户列表
- 租户配置
- 配额管理

**组件**:
- `TenantList` - 租户列表
- `TenantConfig` - 租户配置
- `QuotaManager` - 配额管理

---

## 状态管理

### Zustand Store

```typescript
// stores/auth.ts
import { create } from 'zustand';

interface AuthState {
  token: string | null;
  tenantId: string | null;
  user: User | null;
  setToken: (token: string) => void;
  logout: () => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  token: null,
  tenantId: null,
  user: null,
  setToken: (token) => set({ token }),
  logout: () => set({ token: null, user: null }),
}));
```

### TanStack Query

用于服务器状态管理：

```typescript
// hooks/useCredentials.ts
import { useQuery, useMutation } from '@tanstack/react-query';

export function useCredentials() {
  return useQuery({
    queryKey: ['credentials'],
    queryFn: fetchCredentials,
    staleTime: 5 * 60 * 1000, // 5 分钟
  });
}

export function useCreateCredential() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: createCredential,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] });
    },
  });
}
```

---

## API 集成

### API 客户端

```typescript
// shared/api/client.ts
import axios from 'axios';

const apiClient = axios.create({
  baseURL: import.meta.env.VITE_API_URL || '/api/v1',
  headers: {
    'Content-Type': 'application/json',
  },
});

// 请求拦截器 - 添加 Token
apiClient.interceptors.request.use((config) => {
  const token = useAuthStore.getState().token;
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

// 响应拦截器 - 错误处理
apiClient.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      useAuthStore.getState().logout();
    }
    return Promise.reject(error);
  }
);
```

### API Hooks

```typescript
// features/credentials/api.ts
export async function fetchCredentials(): Promise<Credential[]> {
  const response = await apiClient.get('/credentials');
  return response.data;
}

export async function createCredential(
  data: CreateCredentialRequest
): Promise<Credential> {
  const response = await apiClient.post('/credentials', data);
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

| 组件 | 用途 |
|------|------|
| `Button` | 按钮 |
| `Card` | 卡片容器 |
| `Dialog` | 弹窗对话框 |
| `Form` | 表单处理 |
| `Input` | 文本输入 |
| `Select` | 下拉选择 |
| `Table` | 数据表格 |
| `Tabs` | 标签页 |
| `Toast` | 消息提示 |
| `DropdownMenu` | 下拉菜单 |
| `Tooltip` | 工具提示 |
| `Progress` | 进度条 |
| `ScrollArea` | 滚动区域 |
| `Separator` | 分隔线 |
| `Avatar` | 头像 |
| `Badge` | 徽章 |
| `AlertDialog` | 确认对话框 |
| `Accordion` | 手风琴 |
| `Collapsible` | 可折叠 |
| `Label` | 标签 |

### 组件示例

```tsx
// features/credentials/components/CredentialCard.tsx
import { Card, CardHeader, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';

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
          <Badge variant={credential.is_deleted ? 'destructive' : 'default'}>
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
import { createBrowserRouter } from 'react-router-dom';

export const router = createBrowserRouter([
  {
    path: '/',
    element: <Layout />,
    children: [
      { index: true, element: <Dashboard /> },
      { path: 'credentials', element: <CredentialsPage /> },
      { path: 'credentials/:id', element: <CredentialDetailPage /> },
      { path: 'audit', element: <AuditPage /> },
      { path: 'tenants', element: <TenantsPage /> },
      { path: 'tokens', element: <TokensPage /> },
    ],
  },
  { path: '/login', element: <LoginPage /> },
]);
```

---

## 样式系统

### Tailwind 配置

```javascript
// tailwind.config.js
module.exports = {
  darkMode: ['class'],
  content: ['./src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        border: 'hsl(var(--border))',
        input: 'hsl(var(--input))',
        ring: 'hsl(var(--ring))',
        background: 'hsl(var(--background))',
        foreground: 'hsl(var(--foreground))',
        primary: {
          DEFAULT: 'hsl(var(--primary))',
          foreground: 'hsl(var(--primary-foreground))',
        },
        // ... 更多颜色
      },
      fontFamily: {
        sans: ['Geist Variable', 'system-ui', 'sans-serif'],
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
import { useState } from 'react';

// 2. Types
interface Props {
  title: string;
}

// 3. Component
export function MyComponent({ title }: Props) {
  // State
  const [count, setCount] = useState(0);

  // Handlers
  const handleClick = () => setCount(c => c + 1);

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

*本文档由 BMAD document-project 工作流自动生成*
