# CredBridge Web 前端 - 架构设计文档

---

## 1. 文档信息

| 属性 | 值 |
|-----|-----|
| 文档名称 | CredBridge Web Console 架构设计 |
| 版本 | 1.0 |
| 状态 | Draft |
| 创建日期 | 2026-03-11 |
| 负责人 | claude_kimi |
| 关联文档 | [PRD](./frontend-prd.md), [产品简报](./frontend-brief.md) |

---

## 2. 架构概述

### 2.1 设计原则

1. **安全优先**: 所有设计决策以安全为首要考虑
2. **类型安全**: 全链路 TypeScript 严格模式
3. **模块化**: 高内聚、低耦合的组件设计
4. **可测试**: 易于单元测试和 E2E 测试
5. **性能**: 懒加载、代码分割、虚拟滚动

### 2.2 技术栈决策

| 层级 | 技术选型 | 理由 |
|-----|---------|------|
| 框架 | React 18.3 | 生态成熟，并发特性 |
| 语言 | TypeScript 5.4 | 类型安全，开发体验好 |
| 构建 | Vite 5 | 快速 HMR，优化构建 |
| UI 组件 | shadcn/ui + Radix | 可定制，无障碍支持好 |
| 样式 | Tailwind CSS 3.4 | 原子化，开发效率高 |
| 状态管理 | Zustand 4.5 | 简洁，TypeScript 友好 |
| 数据获取 | TanStack Query 5 | 缓存、乐观更新、离线支持 |
| 路由 | React Router 6.22 | 标准路由解决方案 |
| 表单 | React Hook Form + Zod | 性能优秀，验证强大 |
| 测试 | Vitest + React Testing Library | 快速，符合测试理念 |
| HTTP 客户端 | Axios | 拦截器、取消请求 |

---

## 3. 系统架构

### 3.1 整体架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                        Browser                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
│  │   Login     │  │  Dashboard  │  │ Credentials │             │
│  │    Page     │  │    Page     │  │    Page     │             │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘             │
│         └─────────────────┼─────────────────┘                   │
│                           │                                     │
│  ┌────────────────────────┴────────────────────────┐           │
│  │              React Router v6                     │           │
│  │         (路由守卫 + 懒加载)                       │           │
│  └────────────────────────┬────────────────────────┘           │
│                           │                                     │
│  ┌────────────────────────┴────────────────────────┐           │
│  │           Feature Modules                        │           │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌────────┐ │           │
│  │  │  Auth   │ │ Credential│ │  Token  │ │ Audit  │ │           │
│  │  │ Module  │ │  Module   │ │ Module  │ │ Module │ │           │
│  │  └────┬────┘ └────┬────┘ └────┬────┘ └───┬────┘ │           │
│  │       └───────────┴───────────┴──────────┘       │           │
│  └────────────────────────┬────────────────────────┘           │
│                           │                                     │
│  ┌────────────────────────┴────────────────────────┐           │
│  │           Shared Layer                           │           │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌────────┐ │           │
│  │  │  API    │ │  Store  │ │  Hooks  │ │ Utils  │ │           │
│  │  │ Client  │ │(Zustand)│ │(Shared) │ │(Shared)│ │           │
│  │  └─────────┘ └─────────┘ └─────────┘ └────────┘ │           │
│  └─────────────────────────────────────────────────┘           │
│                           │                                     │
│                           ▼                                     │
│              ┌─────────────────────┐                           │
│              │   CredBridge API    │                           │
│              │   (RESTful + SSE)   │                           │
│              └─────────────────────┘                           │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 模块划分

```
src/
├── features/                    # 功能模块 (按领域划分)
│   ├── auth/                    # 认证模块
│   │   ├── api/
│   │   ├── components/
│   │   ├── hooks/
│   │   ├── stores/
│   │   ├── types/
│   │   └── utils/
│   ├── credentials/             # 凭证模块
│   ├── tokens/                  # Token 模块
│   ├── audit/                   # 审计模块
│   ├── tenants/                 # 租户模块
│   └── layout/                  # 布局模块
│
├── shared/                      # 共享层
│   ├── api/                     # API 客户端
│   ├── components/              # 通用组件
│   ├── hooks/                   # 通用 Hooks
│   ├── lib/                     # 工具库
│   ├── stores/                  # 全局状态
│   ├── styles/                  # 全局样式
│   ├── types/                   # 全局类型
│   └── utils/                   # 工具函数
│
├── app/                         # 应用层
│   ├── router.tsx               # 路由配置
│   ├── providers.tsx            # 全局 Provider
│   └── App.tsx                  # 根组件
│
└── main.tsx                     # 入口文件
```

---

## 4. 详细设计

### 4.1 路由设计

```typescript
// src/app/router.tsx
import { createBrowserRouter, Navigate } from 'react-router-dom';
import { lazy, Suspense } from 'react';
import { ProtectedRoute } from '@/features/auth/components/ProtectedRoute';
import { MainLayout } from '@/features/layout/components/MainLayout';

// 懒加载页面
const LoginPage = lazy(() => import('@/features/auth/pages/LoginPage'));
const DashboardPage = lazy(() => import('@/features/dashboard/pages/DashboardPage'));
const CredentialsPage = lazy(() => import('@/features/credentials/pages/CredentialsPage'));
const CredentialDetailPage = lazy(() => import('@/features/credentials/pages/CredentialDetailPage'));
const TokensPage = lazy(() => import('@/features/tokens/pages/TokensPage'));
const AuditPage = lazy(() => import('@/features/audit/pages/AuditPage'));
const SettingsPage = lazy(() => import('@/features/tenants/pages/SettingsPage'));
const UsersPage = lazy(() => import('@/features/tenants/pages/UsersPage'));
const ProfilePage = lazy(() => import('@/features/auth/pages/ProfilePage'));

export const router = createBrowserRouter([
  {
    path: '/login',
    element: (
      <Suspense fallback={<PageLoading />}>
        <LoginPage />
      </Suspense>
    ),
  },
  {
    path: '/',
    element: (
      <ProtectedRoute>
        <MainLayout />
      </ProtectedRoute>
    ),
    children: [
      { index: true, element: <Navigate to="/dashboard" replace /> },
      {
        path: 'dashboard',
        element: (
          <Suspense fallback={<PageLoading />}>
            <DashboardPage />
          </Suspense>
        ),
      },
      {
        path: 'credentials',
        children: [
          {
            index: true,
            element: (
              <Suspense fallback={<PageLoading />}>
                <CredentialsPage />
              </Suspense>
            ),
          },
          {
            path: ':id',
            element: (
              <Suspense fallback={<PageLoading />}>
                <CredentialDetailPage />
              </Suspense>
            ),
          },
        ],
      },
      {
        path: 'tokens',
        element: (
          <Suspense fallback={<PageLoading />}>
            <TokensPage />
          </Suspense>
        ),
      },
      {
        path: 'audit',
        element: (
          <Suspense fallback={<PageLoading />}>
            <AuditPage />
          </Suspense>
        ),
      },
      {
        path: 'settings',
        element: (
          <Suspense fallback={<PageLoading />}>
            <SettingsPage />
          </Suspense>
        ),
      },
      {
        path: 'users',
        element: (
          <Suspense fallback={<PageLoading />}>
            <UsersPage />
          </Suspense>
        ),
      },
      {
        path: 'profile',
        element: (
          <Suspense fallback={<PageLoading />}>
            <ProfilePage />
          </Suspense>
        ),
      },
    ],
  },
  {
    path: '*',
    element: <NotFoundPage />,
  },
]);
```

### 4.2 状态管理

#### 4.2.1 全局状态 (Zustand)

```typescript
// src/shared/stores/authStore.ts
import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type { User } from '@/features/auth/types';

interface AuthState {
  user: User | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  theme: 'light' | 'dark' | 'system';

  // Actions
  setUser: (user: User | null) => void;
  setAuthenticated: (value: boolean) => void;
  setTheme: (theme: 'light' | 'dark' | 'system') => void;
  logout: () => void;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      user: null,
      isAuthenticated: false,
      isLoading: true,
      theme: 'system',

      setUser: (user) => set({ user, isAuthenticated: !!user }),
      setAuthenticated: (value) => set({ isAuthenticated: value }),
      setTheme: (theme) => set({ theme }),
      logout: () => set({ user: null, isAuthenticated: false }),
    }),
    {
      name: 'auth-storage',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ theme: state.theme }),
    }
  )
);
```

#### 4.2.2 服务端状态 (TanStack Query)

```typescript
// src/features/credentials/api/queries.ts
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { credentialsApi } from './credentialsApi';
import type { Credential, CreateCredentialDTO } from '../types';

// Query Keys
export const credentialKeys = {
  all: ['credentials'] as const,
  lists: () => [...credentialKeys.all, 'list'] as const,
  list: (filters: Record<string, unknown>) =>
    [...credentialKeys.lists(), filters] as const,
  details: () => [...credentialKeys.all, 'detail'] as const,
  detail: (id: string) => [...credentialKeys.details(), id] as const,
};

// Queries
export const useCredentials = (filters?: Record<string, unknown>) => {
  return useQuery({
    queryKey: credentialKeys.list(filters ?? {}),
    queryFn: () => credentialsApi.getList(filters),
  });
};

export const useCredential = (id: string) => {
  return useQuery({
    queryKey: credentialKeys.detail(id),
    queryFn: () => credentialsApi.getById(id),
    enabled: !!id,
  });
};

// Mutations
export const useCreateCredential = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateCredentialDTO) =>
      credentialsApi.create(data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: credentialKeys.lists(),
      });
    },
  });
};

export const useUpdateCredential = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<CreateCredentialDTO> }) =>
      credentialsApi.update(id, data),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: credentialKeys.detail(variables.id),
      });
      queryClient.invalidateQueries({
        queryKey: credentialKeys.lists(),
      });
    },
  });
};

export const useDeleteCredential = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => credentialsApi.delete(id),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: credentialKeys.lists(),
      });
    },
  });
};
```

### 4.3 API 客户端

```typescript
// src/shared/api/client.ts
import axios, { AxiosError, type InternalAxiosRequestConfig } from 'axios';
import { useAuthStore } from '@/shared/stores/authStore';

// API 基础配置
const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:8080/api/v1';

// 创建 axios 实例
export const apiClient = axios.create({
  baseURL: API_BASE_URL,
  timeout: 30000,
  headers: {
    'Content-Type': 'application/json',
  },
});

// 请求拦截器
apiClient.interceptors.request.use(
  (config: InternalAxiosRequestConfig) => {
    // 添加认证头
    const token = localStorage.getItem('access_token');
    if (token) {
      config.headers.Authorization = `Bearer ${token}`;
    }

    // 添加请求 ID 用于追踪
    config.headers['X-Request-ID'] = crypto.randomUUID();

    return config;
  },
  (error) => Promise.reject(error)
);

// 响应拦截器
apiClient.interceptors.response.use(
  (response) => response,
  async (error: AxiosError) => {
    const originalRequest = error.config as InternalAxiosRequestConfig & { _retry?: boolean };

    // 401 未授权处理
    if (error.response?.status === 401 && !originalRequest._retry) {
      originalRequest._retry = true;

      try {
        // 尝试刷新 token
        const refreshToken = localStorage.getItem('refresh_token');
        const response = await axios.post(`${API_BASE_URL}/auth/refresh`, {
          refresh_token: refreshToken,
        });

        const { access_token, refresh_token } = response.data;
        localStorage.setItem('access_token', access_token);
        localStorage.setItem('refresh_token', refresh_token);

        // 重试原请求
        originalRequest.headers.Authorization = `Bearer ${access_token}`;
        return apiClient(originalRequest);
      } catch (refreshError) {
        // 刷新失败，登出用户
        useAuthStore.getState().logout();
        window.location.href = '/login';
        return Promise.reject(refreshError);
      }
    }

    // 统一错误处理
    return Promise.reject(handleApiError(error));
  }
);

// 错误处理函数
function handleApiError(error: AxiosError) {
  if (error.response) {
    const { status, data } = error.response;
    const apiError = data as { code?: string; message?: string; details?: unknown };

    return {
      type: 'api',
      status,
      code: apiError.code || 'UNKNOWN_ERROR',
      message: apiError.message || '发生未知错误',
      details: apiError.details,
    };
  }

  if (error.request) {
    return {
      type: 'network',
      code: 'NETWORK_ERROR',
      message: '网络连接失败，请检查网络',
    };
  }

  return {
    type: 'unknown',
    code: 'UNKNOWN_ERROR',
    message: error.message || '发生未知错误',
  };
}

export type ApiError = ReturnType<typeof handleApiError>;
```

### 4.4 组件设计

#### 4.4.1 组件分层

```
components/
├── ui/                    # 基础 UI 组件 (shadcn/ui)
│   ├── button.tsx
│   ├── input.tsx
│   ├── dialog.tsx
│   └── ...
│
├── composite/             # 组合组件
│   ├── data-table/        # 数据表格
│   │   ├── DataTable.tsx
│   │   ├── DataTablePagination.tsx
│   │   ├── DataTableSorting.tsx
│   │   └── DataTableFiltering.tsx
│   ├── form/              # 表单组件
│   │   ├── FormField.tsx
│   │   ├── FormPassword.tsx
│   │   └── FormSelect.tsx
│   └── feedback/          # 反馈组件
│       ├── ErrorBoundary.tsx
│       ├── Loading.tsx
│       └── EmptyState.tsx
│
└── layout/                # 布局组件
    ├── AppSidebar.tsx
    ├── AppHeader.tsx
    └── AppShell.tsx
```

#### 4.4.2 数据表格组件示例

```typescript
// src/shared/components/data-table/DataTable.tsx
import {
  useReactTable,
  getCoreRowModel,
  getPaginationRowModel,
  getSortingRowModel,
  getFilteredRowModel,
  flexRender,
  type ColumnDef,
  type SortingState,
  type ColumnFiltersState,
} from '@tanstack/react-table';
import { useState } from 'react';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/shared/components/ui/table';
import { DataTablePagination } from './DataTablePagination';
import { DataTableSorting } from './DataTableSorting';

interface DataTableProps<TData, TValue> {
  columns: ColumnDef<TData, TValue>[];
  data: TData[];
  isLoading?: boolean;
  totalCount?: number;
  onPaginationChange?: (page: number, pageSize: number) => void;
  onSortingChange?: (sorting: SortingState) => void;
}

export function DataTable<TData, TValue>({
  columns,
  data,
  isLoading,
  totalCount,
  onPaginationChange,
  onSortingChange,
}: DataTableProps<TData, TValue>) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);

  const table = useReactTable({
    data,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    getSortingRowModel: getSortingRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    onSortingChange: (updater) => {
      const newSorting = typeof updater === 'function' ? updater(sorting) : updater;
      setSorting(newSorting);
      onSortingChange?.(newSorting);
    },
    onColumnFiltersChange: setColumnFilters,
    state: {
      sorting,
      columnFilters,
    },
  });

  if (isLoading) {
    return <DataTableSkeleton />;
  }

  return (
    <div className="space-y-4">
      <div className="rounded-md border">
        <Table>
          <TableHeader>
            {table.getHeaderGroups().map((headerGroup) => (
              <TableRow key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <TableHead key={header.id}>
                    {header.isPlaceholder
                      ? null
                      : flexRender(
                          header.column.columnDef.header,
                          header.getContext()
                        )}
                  </TableHead>
                ))}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody>
            {table.getRowModel().rows?.length ? (
              table.getRowModel().rows.map((row) => (
                <TableRow
                  key={row.id}
                  data-state={row.getIsSelected() && 'selected'}
                >
                  {row.getVisibleCells().map((cell) => (
                    <TableCell key={cell.id}>
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </TableCell>
                  ))}
                </TableRow>
              ))
            ) : (
              <TableRow>
                <TableCell colSpan={columns.length} className="h-24 text-center">
                  暂无数据
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      <DataTablePagination
        table={table}
        totalCount={totalCount}
        onChange={onPaginationChange}
      />
    </div>
  );
}
```

### 4.5 主题系统

```typescript
// src/shared/styles/theme.ts
export const theme = {
  colors: {
    // 深色主题
    dark: {
      background: '#0f0f0f',
      surface: '#1a1a1a',
      'surface-hover': '#252525',
      border: '#2a2a2a',
      'border-hover': '#3a3a3a',
      primary: '#3b82f6',
      'primary-hover': '#2563eb',
      text: '#ffffff',
      'text-secondary': '#a0a0a0',
      'text-tertiary': '#6a6a6a',
      error: '#ef4444',
      warning: '#f59e0b',
      success: '#22c55e',
    },
    // 浅色主题
    light: {
      background: '#f5f5f5',
      surface: '#ffffff',
      'surface-hover': '#f9fafb',
      border: '#e0e0e0',
      'border-hover': '#d1d5db',
      primary: '#3b82f6',
      'primary-hover': '#2563eb',
      text: '#1a1a1a',
      'text-secondary': '#666666',
      'text-tertiary': '#9ca3af',
      error: '#dc2626',
      warning: '#d97706',
      success: '#16a34a',
    },
  },
  spacing: {
    xs: '0.25rem',    // 4px
    sm: '0.5rem',     // 8px
    md: '1rem',       // 16px
    lg: '1.5rem',     // 24px
    xl: '2rem',       // 32px
    '2xl': '3rem',    // 48px
  },
  borderRadius: {
    sm: '0.25rem',
    md: '0.5rem',
    lg: '0.75rem',
    xl: '1rem',
  },
  shadows: {
    sm: '0 1px 2px 0 rgba(0, 0, 0, 0.05)',
    md: '0 4px 6px -1px rgba(0, 0, 0, 0.1)',
    lg: '0 10px 15px -3px rgba(0, 0, 0, 0.1)',
  },
} as const;

// CSS 变量注入
export function injectThemeVariables(theme: 'light' | 'dark') {
  const colors = theme === 'light' ? theme.colors.light : theme.colors.dark;
  const root = document.documentElement;

  Object.entries(colors).forEach(([key, value]) => {
    root.style.setProperty(`--color-${key}`, value);
  });
}
```

---

## 5. 安全设计

### 5.1 安全策略

| 层级 | 措施 |
|-----|------|
| 传输层 | 强制 HTTPS，TLS 1.3 |
| 认证层 | JWT + HttpOnly Cookie，自动刷新 |
| 应用层 | CSP 策略，XSS 过滤，CSRF Token |
| 数据层 | 敏感字段加密存储，内存安全 |

### 5.2 CSP 配置

```typescript
// index.html 中配置 CSP
const cspPolicy = [
  "default-src 'self'",
  "script-src 'self' 'unsafe-inline'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: https:",
  "font-src 'self'",
  "connect-src 'self'",
  "frame-ancestors 'none'",
  "base-uri 'self'",
  "form-action 'self'",
].join('; ');

// 通过 meta 标签或 HTTP Header 注入
```

### 5.3 敏感数据处理

```typescript
// src/shared/utils/sensitive.ts

/**
 * 安全清除内存中的敏感数据
 */
export function secureClear(data: string | Uint8Array): void {
  if (typeof data === 'string') {
    // 覆盖字符串（JavaScript 中无法真正清除，但做最佳尝试）
    const arr = new Uint8Array(data.length);
    arr.fill(0);
  } else {
    data.fill(0);
  }
}

/**
 * 脱敏显示敏感信息
 */
export function maskSensitive(
  value: string,
  options: { start?: number; end?: number; mask?: string } = {}
): string {
  const { start = 2, end = 2, mask = '•' } = options;

  if (value.length <= start + end) {
    return mask.repeat(value.length);
  }

  const visibleStart = value.slice(0, start);
  const visibleEnd = value.slice(-end);
  const maskedLength = value.length - start - end;

  return `${visibleStart}${mask.repeat(maskedLength)}${visibleEnd}`;
}

// 示例: maskSensitive('sk-abc123xyz', { start: 3, end: 3 }) => 'sk-••••••xyz'
```

---

## 6. 性能优化

### 6.1 加载优化

| 策略 | 实现 |
|-----|------|
| 代码分割 | 按路由懒加载 |
| 资源预加载 | 关键资源 preload |
| Tree Shaking | ESM 模块化 |
| Gzip/Brotli | 服务端压缩 |

### 6.2 运行时优化

| 策略 | 实现 |
|-----|------|
| 虚拟滚动 | 大数据列表使用 react-window |
| 防抖节流 | 搜索输入、窗口调整 |
| 记忆化 | React.memo, useMemo, useCallback |
| 缓存策略 | TanStack Query 智能缓存 |

---

## 7. 测试策略

### 7.1 测试金字塔

```
       /\
      /  \     E2E Tests (Playwright)
     /____\
    /      \   Integration Tests
   /________\
  /          \ Unit Tests (Vitest)
 /____________\
```

### 7.2 测试配置

```typescript
// vitest.config.ts
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: [
        'node_modules/',
        'src/test/',
        '**/*.d.ts',
        '**/*.config.*',
      ],
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
});
```

---

## 8. 部署架构

### 8.1 构建输出

```
dist/
├── assets/
│   ├── index-[hash].js
│   ├── index-[hash].css
│   ├── vendor-[hash].js
│   └── ...
├── index.html
└── _headers  # Netlify/Vercel 配置
```

### 8.2 部署配置

```nginx
# Nginx 配置示例
server {
    listen 443 ssl http2;
    server_name console.credbridge.io;

    # SSL 配置
    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    ssl_protocols TLSv1.3;

    # 安全头
    add_header X-Frame-Options "DENY" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;

    # CSP
    add_header Content-Security-Policy "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';" always;

    # 静态资源
    location / {
        root /var/www/credbridge-console;
        try_files $uri $uri/ /index.html;

        # 缓存策略
        location ~* \.(js|css|png|jpg|jpeg|gif|ico|svg|woff|woff2)$ {
            expires 1y;
            add_header Cache-Control "public, immutable";
        }
    }

    # API 代理
    location /api/ {
        proxy_pass http://backend:8080/api/;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

---

## 9. 项目结构

### 9.1 目录结构

```
credbridge/frontend/
├── public/                      # 静态资源
│   ├── favicon.ico
│   └── logo.svg
│
├── src/
│   ├── app/                     # 应用层
│   │   ├── App.tsx
│   │   ├── router.tsx
│   │   └── providers.tsx
│   │
│   ├── features/                # 功能模块
│   │   ├── auth/
│   │   │   ├── api/
│   │   │   │   ├── authApi.ts
│   │   │   │   └── queries.ts
│   │   │   ├── components/
│   │   │   │   ├── LoginForm.tsx
│   │   │   │   ├── MFASetup.tsx
│   │   │   │   └── ProtectedRoute.tsx
│   │   │   ├── hooks/
│   │   │   │   └── useAuth.ts
│   │   │   ├── pages/
│   │   │   │   ├── LoginPage.tsx
│   │   │   │   └── ProfilePage.tsx
│   │   │   ├── stores/
│   │   │   │   └── authStore.ts
│   │   │   ├── types/
│   │   │   │   └── index.ts
│   │   │   └── utils/
│   │   │       └── authUtils.ts
│   │   │
│   │   ├── credentials/
│   │   ├── tokens/
│   │   ├── audit/
│   │   ├── tenants/
│   │   ├── dashboard/
│   │   └── layout/
│   │
│   ├── shared/                  # 共享层
│   │   ├── api/
│   │   │   └── client.ts
│   │   ├── components/
│   │   │   └── ui/              # shadcn/ui 组件
│   │   ├── hooks/
│   │   ├── lib/
│   │   │   └── utils.ts
│   │   ├── stores/
│   │   ├── styles/
│   │   ├── types/
│   │   └── utils/
│   │
│   └── main.tsx
│
├── .env.example
├── .eslintrc.cjs
├── .gitignore
├── components.json              # shadcn/ui 配置
├── index.html
├── package.json
├── postcss.config.js
├── tailwind.config.ts
├── tsconfig.json
└── vitest.config.ts
```

### 9.2 依赖列表

```json
{
  "dependencies": {
    "react": "^18.3.0",
    "react-dom": "^18.3.0",
    "react-router-dom": "^6.22.0",
    "@tanstack/react-query": "^5.24.0",
    "axios": "^1.6.7",
    "zustand": "^4.5.1",
    "react-hook-form": "^7.51.0",
    "zod": "^3.22.4",
    "@hookform/resolvers": "^3.3.4",
    "@radix-ui/react-dialog": "^1.0.5",
    "@radix-ui/react-dropdown-menu": "^2.0.6",
    "@radix-ui/react-select": "^2.0.0",
    "@radix-ui/react-toast": "^1.1.5",
    "class-variance-authority": "^0.7.0",
    "clsx": "^2.1.0",
    "tailwind-merge": "^2.2.1",
    "lucide-react": "^0.344.0",
    "date-fns": "^3.3.1",
    "qrcode.react": "^3.1.0"
  },
  "devDependencies": {
    "@types/react": "^18.2.61",
    "@types/react-dom": "^18.2.19",
    "@typescript-eslint/eslint-plugin": "^7.1.0",
    "@typescript-eslint/parser": "^7.1.0",
    "@vitejs/plugin-react": "^4.2.1",
    "autoprefixer": "^10.4.18",
    "eslint": "^8.57.0",
    "eslint-plugin-react-hooks": "^4.6.0",
    "eslint-plugin-react-refresh": "^0.4.5",
    "jsdom": "^24.0.0",
    "postcss": "^8.4.35",
    "tailwindcss": "^3.4.1",
    "typescript": "^5.4.0",
    "vite": "^5.1.4",
    "vitest": "^1.3.1",
    "@testing-library/react": "^14.2.1",
    "@testing-library/jest-dom": "^6.4.2"
  }
}
```

---

## 10. 开发规范

### 10.1 代码规范

- **ESLint**: 使用推荐配置 + TypeScript 规则
- **Prettier**: 统一代码格式
- **命名规范**:
  - 组件: PascalCase
  - Hooks: camelCase (useXxx)
  - 工具函数: camelCase
  - 常量: UPPER_SNAKE_CASE
  - 类型: PascalCase (Type/Interface)

### 10.2 提交规范

```
<type>(<scope>): <subject>

<body>

<footer>
```

类型:
- `feat`: 新功能
- `fix`: 修复
- `docs`: 文档
- `style`: 格式
- `refactor`: 重构
- `test`: 测试
- `chore`: 构建/工具

---

## 11. 待决策事项

| 事项 | 选项 | 建议 | 决策状态 |
|-----|------|------|---------|
| 国际化 | react-i18next / 暂不 | 暂不，V1.1 考虑 | ⏳ |
| 图表库 | Recharts / Victory / 暂不 | 暂不，V1.1 考虑 | ⏳ |
| 文档工具 | Storybook / 暂不 | 暂不 | ⏳ |
| SSR | 需要 / 不需要 | 不需要，纯 CSR | ✅ |

---

## 12. 附录

### 12.1 变更日志

| 版本 | 日期 | 变更内容 | 作者 |
|-----|------|---------|------|
| 0.1 | 2026-03-11 | 初始版本 | claude_kimi |

### 12.2 参考文档

- [PRD](./frontend-prd.md)
- [产品简报](./frontend-brief.md)
- [shadcn/ui 文档](https://ui.shadcn.com)
- [TanStack Query 文档](https://tanstack.com/query)

---

**文档状态**: Draft
**下次评审**: Epic 分解阶段
