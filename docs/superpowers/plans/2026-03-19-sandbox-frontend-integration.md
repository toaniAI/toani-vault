# 沙箱功能前端集成计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将后端沙箱功能添加到前端 SDK 和 API 文档页面

**Architecture:** 复用现有的 DeveloperCenter 页面架构，在 SDK 类型定义、API 服务、API 文档展示中添加沙箱相关功能，保持与现有凭证管理、Token 管理等功能的一致性。

**Tech Stack:** React 19, TypeScript, TanStack Query, shadcn/ui

---

## 文件结构

| 文件 | 职责 | 操作 |
|------|------|------|
| `/frontend/src/shared/api/types.ts` | SDK 类型定义 | 修改 - 添加沙箱相关接口 |
| `/frontend/src/shared/api/services.ts` | API 服务层 | 修改 - 添加 sandboxApi |
| `/frontend/src/shared/api/hooks.ts` | React Query hooks | 修改 - 添加沙箱相关 hooks |
| `/frontend/src/features/developer/pages/DeveloperCenter.tsx` | 开发者中心页面 | 修改 - 添加沙箱 API 文档和示例 |

---

## 探索发现

### 后端沙箱功能（已实现但未在前端展示）

**API 端点**（9个）：
- `POST /api/v1/sandbox/sessions` - 创建会话
- `GET /api/v1/sandbox/sessions` - 列出会话
- `GET /api/v1/sandbox/sessions/:id` - 获取会话详情
- `POST /api/v1/sandbox/sessions/:id/execute` - 执行操作
- `POST /api/v1/sandbox/sessions/:id/pause` - 暂停会话
- `POST /api/v1/sandbox/sessions/:id/resume` - 恢复会话
- `DELETE /api/v1/sandbox/sessions/:id` - 关闭会话
- `POST /api/v1/sandbox/sessions/:id/screenshot` - 截图
- `POST /api/v1/sandbox/sessions/:id/export` - 导出数据

**操作类型**：Navigate, Click, Fill, GetText, Screenshot, Export, ExecuteScript, Wait, Custom

**会话状态**：Creating, Ready, Executing, Paused, Closed

### 当前前端架构

开发者中心页面使用 Tabs 组织内容：
- 系统说明 (SystemOverviewTab)
- API 文档 (ApiDocsTab) - 硬编码的 apiEndpoints 数组
- SDK 示例 (SdkExamplesTab) - 硬编码的代码示例
- API 测试工具 (ApiTesterTab)

---

## Task 1: 添加沙箱类型定义

**Files:**
- Modify: `/frontend/src/shared/api/types.ts`

- [ ] **Step 1: 添加沙箱会话状态枚举**

在文件中添加以下类型定义（找到合适的类型定义区域，通常在文件末尾或相关类型附近）：

```typescript
// 沙箱会话状态
export enum SandboxSessionStatus {
  Creating = 'creating',
  Ready = 'ready',
  Executing = 'executing',
  Paused = 'paused',
  Closed = 'closed',
}

// 沙箱操作类型
export enum SandboxOperationType {
  Navigate = 'navigate',
  Click = 'click',
  Fill = 'fill',
  GetText = 'get_text',
  GetAttribute = 'get_attribute',
  ExecuteScript = 'execute_script',
  WaitForSelector = 'wait_for_selector',
  Screenshot = 'screenshot',
  ExportData = 'export_data',
  Custom = 'custom',
}

// 沙箱会话
export interface SandboxSession {
  id: string;
  tenant_id: string;
  created_by: string;
  status: SandboxSessionStatus;
  started_at: string;
  expires_at: string;
  terminated_at?: string;
  termination_reason?: string;
  tee_context_id?: string;
  security_policy?: Record<string, unknown>;
  metadata?: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

// 创建沙箱会话请求
export interface CreateSandboxSessionRequest {
  credential_id: string;
  security_policy?: {
    max_memory_mb?: number;
    max_cpu_percent?: number;
    max_execution_time_seconds?: number;
    allowed_domains?: string[];
    allow_file_downloads?: boolean;
  };
  metadata?: Record<string, unknown>;
}

// 沙箱操作请求
export interface SandboxOperationRequest {
  operation_type: SandboxOperationType;
  parameters: Record<string, unknown>;
}

// 沙箱操作结果
export interface SandboxOperationResult {
  success: boolean;
  data?: Record<string, unknown>;
  error?: string;
  execution_time_ms?: number;
}

// 沙箱操作记录
export interface SandboxOperation {
  id: string;
  session_id: string;
  operation_type: SandboxOperationType;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  input_params?: Record<string, unknown>;
  output_result?: Record<string, unknown>;
  error_message?: string;
  started_at?: string;
  completed_at?: string;
  execution_duration_ms?: number;
}

// 截图选项
export interface ScreenshotOptions {
  full_page?: boolean;
  selector?: string;
  width?: number;
  height?: number;
  quality?: number;
}

// 截图结果
export interface ScreenshotResult {
  image_data: string; // base64
  format: 'png' | 'jpeg' | 'webp';
  width: number;
  height: number;
}

// 导出数据请求
export interface ExportDataRequest {
  format: 'json' | 'csv' | 'html' | 'pdf';
  selector?: string;
  include_metadata?: boolean;
  redact_sensitive?: boolean;
  apply_watermark?: boolean;
}

// 导出结果
export interface ExportResult {
  download_url: string;
  expires_at: string;
  size_bytes: number;
  format: string;
}
```

- [ ] **Step 2: 验证类型定义无语法错误**

运行: `cd /Users/yvan/AIWorkspace/credbridge/frontend && npx tsc --noEmit`
Expected: 无类型错误

- [ ] **Step 3: Commit**

```bash
git add frontend/src/shared/api/types.ts
git commit -m "feat(api): add sandbox types and interfaces"
```

---

## Task 2: 添加沙箱 API 服务

**Files:**
- Modify: `/frontend/src/shared/api/services.ts`

- [ ] **Step 1: 添加沙箱 API 服务对象**

在文件中添加 sandboxApi（参考现有的 credentialsApi 模式）：

```typescript
export const sandboxApi = {
  // 创建会话
  createSession: (data: CreateSandboxSessionRequest) =>
    apiClient.post<SandboxSession>('/api/v1/sandbox/sessions', data),

  // 列出会话
  listSessions: () =>
    apiClient.get<SandboxSession[]>('/api/v1/sandbox/sessions'),

  // 获取会话详情
  getSession: (sessionId: string) =>
    apiClient.get<SandboxSession>(`/api/v1/sandbox/sessions/${sessionId}`),

  // 执行操作
  executeOperation: (sessionId: string, data: SandboxOperationRequest) =>
    apiClient.post<SandboxOperationResult>(`/api/v1/sandbox/sessions/${sessionId}/execute`, data),

  // 暂停会话
  pauseSession: (sessionId: string) =>
    apiClient.post<void>(`/api/v1/sandbox/sessions/${sessionId}/pause`),

  // 恢复会话
  resumeSession: (sessionId: string) =>
    apiClient.post<void>(`/api/v1/sandbox/sessions/${sessionId}/resume`),

  // 关闭会话
  closeSession: (sessionId: string) =>
    apiClient.delete<void>(`/api/v1/sandbox/sessions/${sessionId}`),

  // 截图
  takeScreenshot: (sessionId: string, options?: ScreenshotOptions) =>
    apiClient.post<ScreenshotResult>(`/api/v1/sandbox/sessions/${sessionId}/screenshot`, options || {}),

  // 导出数据
  exportData: (sessionId: string, data: ExportDataRequest) =>
    apiClient.post<ExportResult>(`/api/v1/sandbox/sessions/${sessionId}/export`, data),

  // 获取操作列表
  listOperations: (sessionId: string) =>
    apiClient.get<SandboxOperation[]>(`/api/v1/sandbox/sessions/${sessionId}/operations`),

  // 获取操作详情
  getOperation: (operationId: string) =>
    apiClient.get<SandboxOperation>(`/api/v1/sandbox/operations/${operationId}`),
};
```

确保导入所需的类型：
```typescript
import type {
  SandboxSession,
  CreateSandboxSessionRequest,
  SandboxOperationRequest,
  SandboxOperationResult,
  SandboxOperation,
  ScreenshotOptions,
  ScreenshotResult,
  ExportDataRequest,
  ExportResult,
} from './types';
```

- [ ] **Step 2: 验证服务无类型错误**

运行: `cd /Users/yvan/AIWorkspace/credbridge/frontend && npx tsc --noEmit`
Expected: 无类型错误

- [ ] **Step 3: Commit**

```bash
git add frontend/src/shared/api/services.ts
git commit -m "feat(api): add sandbox API service methods"
```

---

## Task 3: 添加沙箱 React Query Hooks

**Files:**
- Modify: `/frontend/src/shared/api/hooks.ts`

- [ ] **Step 1: 添加沙箱相关的 React Query hooks**

在文件中添加以下 hooks（参考现有的 useCredentials 模式）：

```typescript
// 沙箱会话相关 Hooks

export function useSandboxSessions() {
  return useQuery({
    queryKey: ['sandbox', 'sessions'],
    queryFn: () => sandboxApi.listSessions(),
    staleTime: 5 * 1000, // 5秒
  });
}

export function useSandboxSession(sessionId: string) {
  return useQuery({
    queryKey: ['sandbox', 'sessions', sessionId],
    queryFn: () => sandboxApi.getSession(sessionId),
    enabled: !!sessionId,
    staleTime: 5 * 1000,
  });
}

export function useCreateSandboxSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: sandboxApi.createSession,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sandbox', 'sessions'] });
    },
  });
}

export function useCloseSandboxSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: sandboxApi.closeSession,
    onSuccess: (_, sessionId) => {
      queryClient.invalidateQueries({ queryKey: ['sandbox', 'sessions'] });
      queryClient.invalidateQueries({ queryKey: ['sandbox', 'sessions', sessionId] });
    },
  });
}

export function usePauseSandboxSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: sandboxApi.pauseSession,
    onSuccess: (_, sessionId) => {
      queryClient.invalidateQueries({ queryKey: ['sandbox', 'sessions', sessionId] });
    },
  });
}

export function useResumeSandboxSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: sandboxApi.resumeSession,
    onSuccess: (_, sessionId) => {
      queryClient.invalidateQueries({ queryKey: ['sandbox', 'sessions', sessionId] });
    },
  });
}

export function useExecuteSandboxOperation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ sessionId, request }: { sessionId: string; request: SandboxOperationRequest }) =>
      sandboxApi.executeOperation(sessionId, request),
    onSuccess: (_, { sessionId }) => {
      queryClient.invalidateQueries({ queryKey: ['sandbox', 'sessions', sessionId, 'operations'] });
    },
  });
}

export function useSandboxOperations(sessionId: string) {
  return useQuery({
    queryKey: ['sandbox', 'sessions', sessionId, 'operations'],
    queryFn: () => sandboxApi.listOperations(sessionId),
    enabled: !!sessionId,
  });
}

export function useTakeScreenshot() {
  return useMutation({
    mutationFn: ({ sessionId, options }: { sessionId: string; options?: ScreenshotOptions }) =>
      sandboxApi.takeScreenshot(sessionId, options),
  });
}

export function useExportSandboxData() {
  return useMutation({
    mutationFn: ({ sessionId, request }: { sessionId: string; request: ExportDataRequest }) =>
      sandboxApi.exportData(sessionId, request),
  });
}
```

确保导入：
```typescript
import { sandboxApi } from './services';
import type {
  CreateSandboxSessionRequest,
  SandboxOperationRequest,
  ScreenshotOptions,
  ExportDataRequest,
} from './types';
```

- [ ] **Step 2: 验证 hooks 无类型错误**

运行: `cd /Users/yvan/AIWorkspace/credbridge/frontend && npx tsc --noEmit`
Expected: 无类型错误

- [ ] **Step 3: Commit**

```bash
git add frontend/src/shared/api/hooks.ts
git commit -m "feat(api): add sandbox React Query hooks"
```

---

## Task 4: 在开发者中心添加沙箱 API 文档

**Files:**
- Modify: `/frontend/src/features/developer/pages/DeveloperCenter.tsx`

- [ ] **Step 1: 在 apiEndpoints 数组中添加沙箱端点**

在文件中现有的 apiEndpoints 数组末尾添加沙箱端点（找到 apiEndpoints 数组定义，添加新条目）：

```typescript
// 在现有 apiEndpoints 数组后添加沙箱端点
const sandboxApiEndpoints = [
  {
    method: 'POST',
    path: '/api/v1/sandbox/sessions',
    description: '创建沙箱浏览器会话，用于安全执行网页操作',
    request: '{
  "credential_id": "uuid",
  "security_policy": {
    "max_memory_mb": 512,
    "max_cpu_percent": 50,
    "max_execution_time_seconds": 300,
    "allowed_domains": ["example.com"],
    "allow_file_downloads": false
  }
}',
    response: '{
  "id": "uuid",
  "status": "ready",
  "started_at": "2024-01-01T00:00:00Z",
  "expires_at": "2024-01-01T00:30:00Z",
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z"
}',
  },
  {
    method: 'GET',
    path: '/api/v1/sandbox/sessions',
    description: '列出当前租户的所有沙箱会话',
    request: '',
    response: '[\n  {\n    "id": "uuid",\n    "status": "ready",\n    "started_at": "2024-01-01T00:00:00Z",\n    "expires_at": "2024-01-01T00:30:00Z"\n  }\n]',
  },
  {
    method: 'GET',
    path: '/api/v1/sandbox/sessions/:id',
    description: '获取指定沙箱会话的详细信息',
    request: '',
    response: '{
  "id": "uuid",
  "status": "ready",
  "started_at": "2024-01-01T00:00:00Z",
  "expires_at": "2024-01-01T00:30:00Z",
  "tee_context_id": "ctx_uuid",
  "security_policy": { ... },
  "metadata": { ... }
}',
  },
  {
    method: 'POST',
    path: '/api/v1/sandbox/sessions/:id/execute',
    description: '在沙箱会话中执行操作（导航、点击、填写等）',
    request: '{
  "operation_type": "navigate",
  "parameters": {
    "url": "https://example.com"
  }
}',
    response: '{
  "success": true,
  "data": {
    "url": "https://example.com",
    "title": "Example Domain"
  },
  "execution_time_ms": 1500
}',
  },
  {
    method: 'POST',
    path: '/api/v1/sandbox/sessions/:id/pause',
    description: '暂停沙箱会话，保留状态但释放资源',
    request: '',
    response: '{ "status": "paused" }',
  },
  {
    method: 'POST',
    path: '/api/v1/sandbox/sessions/:id/resume',
    description: '恢复已暂停的沙箱会话',
    request: '',
    response: '{ "status": "ready" }',
  },
  {
    method: 'DELETE',
    path: '/api/v1/sandbox/sessions/:id',
    description: '关闭并清理沙箱会话',
    request: '',
    response: '{ "message": "Session closed successfully" }',
  },
  {
    method: 'POST',
    path: '/api/v1/sandbox/sessions/:id/screenshot',
    description: '对当前页面截图',
    request: '{
  "full_page": true,
  "quality": 90
}',
    response: '{
  "image_data": "base64_encoded_image_data",
  "format": "png",
  "width": 1920,
  "height": 1080
}',
  },
  {
    method: 'POST',
    path: '/api/v1/sandbox/sessions/:id/export',
    description: '导出页面数据，支持脱敏和水印',
    request: '{
  "format": "json",
  "selector": ".data-table",
  "redact_sensitive": true,
  "apply_watermark": true
}',
    response: '{
  "download_url": "https://api.example.com/exports/xxx",
  "expires_at": "2024-01-01T01:00:00Z",
  "size_bytes": 1024,
  "format": "json"
}',
  },
];

// 合并到现有的 apiEndpoints
const allApiEndpoints = [...apiEndpoints, ...sandboxApiEndpoints];
```

- [ ] **Step 2: 更新 ApiDocsTab 使用合并后的端点列表**

找到 ApiDocsTab 组件中使用 apiEndpoints 的地方，更新为使用 allApiEndpoints：

```typescript
// 搜索类似这样的代码：
{apiEndpoints.map((endpoint, index) => (

// 替换为：
{allApiEndpoints.map((endpoint, index) => (
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/features/developer/pages/DeveloperCenter.tsx
git commit -m "feat(developer): add sandbox API endpoints to documentation"
```

---

## Task 5: 在 SDK 示例中添加沙箱使用示例

**Files:**
- Modify: `/frontend/src/features/developer/pages/DeveloperCenter.tsx`

- [ ] **Step 1: 在 SdkExamplesTab 中添加沙箱 SDK 示例**

找到 SdkExamplesTab 组件，在现有的示例代码后添加沙箱 SDK 示例。添加一个新的 Card 或 AccordionItem：

```tsx
{/* 在现有的示例后添加沙箱示例 */}
<AccordionItem value="sandbox">
  <AccordionTrigger className="text-lg font-semibold">
    <div className="flex items-center gap-2">
      <span>🛡️</span>
      <span>沙箱浏览器自动化</span>
    </div>
  </AccordionTrigger>
  <AccordionContent>
    <div className="space-y-6">
      <div>
        <h4 className="font-semibold mb-2">创建沙箱会话</h4>
        <pre className="bg-muted p-4 rounded-lg overflow-x-auto text-sm">
{`import { CredBridgeSDK } from '@credbridge/sdk';

const sdk = new CredBridgeSDK({
  baseURL: 'https://api.credbridge.io',
  token: 'your-api-token'
});

// 创建沙箱会话
const session = await sdk.sandbox.createSession({
  credential_id: 'your-credential-id',
  security_policy: {
    max_memory_mb: 512,
    max_cpu_percent: 50,
    max_execution_time_seconds: 300,
    allowed_domains: ['example.com'],
    allow_file_downloads: false
  }
});

console.log('Session created:', session.id);
console.log('Status:', session.status); // 'ready'`}
        </pre>
      </div>

      <div>
        <h4 className="font-semibold mb-2">执行浏览器操作</h4>
        <pre className="bg-muted p-4 rounded-lg overflow-x-auto text-sm">
{`// 导航到网页
await sdk.sandbox.navigate(session.id, 'https://example.com/login');

// 填写表单
await sdk.sandbox.fill(session.id, '#username', 'myusername');
await sdk.sandbox.fill(session.id, '#password', 'mypassword');

// 点击按钮
await sdk.sandbox.click(session.id, '#login-btn');

// 等待元素出现
await sdk.sandbox.waitForSelector(session.id, '.dashboard');

// 获取文本内容
const welcomeText = await sdk.sandbox.getText(session.id, '.welcome-message');
console.log('Welcome:', welcomeText);`}
        </pre>
      </div>

      <div>
        <h4 className="font-semibold mb-2">截图和导出数据</h4>
        <pre className="bg-muted p-4 rounded-lg overflow-x-auto text-sm">
{`// 截图
const screenshot = await sdk.sandbox.takeScreenshot(session.id, {
  full_page: true,
  quality: 90
});

// 保存截图
fs.writeFileSync('screenshot.png', Buffer.from(screenshot.image_data, 'base64'));

// 导出数据（带脱敏和水印）
const export = await sdk.sandbox.exportData(session.id, {
  format: 'json',
  selector: '.data-table',
  redact_sensitive: true,
  apply_watermark: true
});

console.log('Download URL:', export.download_url);`}
        </pre>
      </div>

      <div>
        <h4 className="font-semibold mb-2">会话生命周期管理</h4>
        <pre className="bg-muted p-4 rounded-lg overflow-x-auto text-sm">
{`// 暂停会话（保留状态，释放资源）
await sdk.sandbox.pauseSession(session.id);

// 恢复会话
await sdk.sandbox.resumeSession(session.id);

// 获取会话状态
const sessionInfo = await sdk.sandbox.getSession(session.id);
console.log('Status:', sessionInfo.status);

// 列出所有会话
const sessions = await sdk.sandbox.listSessions();

// 关闭会话
await sdk.sandbox.closeSession(session.id);`}
        </pre>
      </div>

      <div>
        <h4 className="font-semibold mb-2">使用 React Hooks</h4>
        <pre className="bg-muted p-4 rounded-lg overflow-x-auto text-sm">
{`import {
  useSandboxSession,
  useCreateSandboxSession,
  useExecuteSandboxOperation,
  useTakeScreenshot
} from '@credbridge/sdk/react';

function SandboxComponent() {
  const { data: session } = useSandboxSession(sessionId);
  const createSession = useCreateSandboxSession();
  const executeOp = useExecuteSandboxOperation();
  const takeScreenshot = useTakeScreenshot();

  const handleCreateSession = async () => {
    const newSession = await createSession.mutateAsync({
      credential_id: 'credential-id'
    });
  };

  const handleNavigate = async (url: string) => {
    await executeOp.mutateAsync({
      sessionId: session.id,
      request: {
        operation_type: 'navigate',
        parameters: { url }
      }
    });
  };

  const handleScreenshot = async () => {
    const result = await takeScreenshot.mutateAsync({
      sessionId: session.id,
      options: { full_page: true }
    });
    return result.image_data;
  };
}`}
        </pre>
      </div>

      <div className="bg-blue-50 dark:bg-blue-950 p-4 rounded-lg">
        <h4 className="font-semibold text-blue-900 dark:text-blue-100 mb-2">安全特性</h4>
        <ul className="list-disc list-inside text-sm text-blue-800 dark:text-blue-200 space-y-1">
          <li>TEE 可信执行环境保护会话执行</li>
          <li>AI 审核引擎自动检测可疑操作</li>
          <li>数据导出自动脱敏和加水印</li>
          <li>资源限制（CPU、内存、执行时间）</li>
          <li>域名白名单限制访问范围</li>
          <li>Linux Namespace 和 Seccomp 沙箱隔离</li>
        </ul>
      </div>
    </div>
  </AccordionContent>
</AccordionItem>
```

- [ ] **Step 2: 验证组件无语法错误**

运行: `cd /Users/yvan/AIWorkspace/credbridge/frontend && npx tsc --noEmit`
Expected: 无类型错误

- [ ] **Step 3: Commit**

```bash
git add frontend/src/features/developer/pages/DeveloperCenter.tsx
git commit -m "feat(developer): add sandbox SDK examples to documentation"
```

---

## Task 6: 添加沙箱系统说明到系统概览

**Files:**
- Modify: `/frontend/src/features/developer/pages/DeveloperCenter.tsx`

- [ ] **Step 1: 在 SystemOverviewTab 中添加沙箱架构说明**

找到 SystemOverviewTab 组件，在现有的安全特性或功能模块后添加沙箱说明。添加一个新的 Card：

```tsx
{/* 在系统概览中添加沙箱架构说明 */}
<Card>
  <CardHeader>
    <CardTitle className="flex items-center gap-2">
      <span>🛡️</span>
      <span>TEE 沙箱浏览器</span>
    </CardTitle>
    <CardDescription>
      安全隔离的网页自动化执行环境
    </CardDescription>
  </CardHeader>
  <CardContent>
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        沙箱浏览器在可信执行环境（TEE）中运行，为凭证验证和数据采集提供安全隔离的执行环境。
        每个沙箱会话都在独立的 Linux Namespace 中运行，受到 Seccomp 系统调用过滤和 cgroup 资源限制的保护。
      </p>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div>
          <h4 className="font-semibold mb-2">核心能力</h4>
          <ul className="list-disc list-inside text-sm text-muted-foreground space-y-1">
            <li>网页导航和表单填写</li>
            <li>元素点击和文本提取</li>
            <li>JavaScript 脚本执行</li>
            <li>页面截图（支持全页面）</li>
            <li>数据导出（JSON/CSV/HTML/PDF）</li>
            <li>会话暂停和恢复</li>
          </ul>
        </div>

        <div>
          <h4 className="font-semibold mb-2">安全机制</h4>
          <ul className="list-disc list-inside text-sm text-muted-foreground space-y-1">
            <li>TEE 可信执行环境保护</li>
            <li>AI 实时操作审核</li>
            <li>Linux Namespace 隔离</li>
            <li>Seccomp 系统调用过滤</li>
            <li>cgroup 资源限制</li>
            <li>数据脱敏和水印</li>
          </ul>
        </div>
      </div>

      <div className="bg-muted p-3 rounded-lg text-sm">
        <strong>典型应用场景：</strong>
        凭证验证、数据爬取、自动化测试、敏感数据获取（银行账单、医疗记录等）
      </div>
    </div>
  </CardContent>
</Card>
```

- [ ] **Step 2: 验证组件无语法错误**

运行: `cd /Users/yvan/AIWorkspace/credbridge/frontend && npx tsc --noEmit`
Expected: 无类型错误

- [ ] **Step 3: Commit**

```bash
git add frontend/src/features/developer/pages/DeveloperCenter.tsx
git commit -m "feat(developer): add sandbox architecture overview"
```

---

## Task 7: 最终验证

**Files:**
- All modified files

- [ ] **Step 1: 完整类型检查**

运行: `cd /Users/yvan/AIWorkspace/credbridge/frontend && npx tsc --noEmit`
Expected: 无类型错误

- [ ] **Step 2: 构建前端**

运行: `cd /Users/yvan/AIWorkspace/credbridge/frontend && npm run build`
Expected: 构建成功，无错误

- [ ] **Step 3: 最终 Commit（如有必要）**

```bash
git log --oneline -10  # 查看已有的提交
```

如果所有更改都已正确提交，此步骤可跳过。

---

## 验证清单

实现完成后，请确认以下功能已正确添加：

- [ ] TypeScript 类型定义包含所有沙箱相关接口
- [ ] API 服务层包含所有沙箱 API 方法
- [ ] React Query hooks 包含所有沙箱相关操作
- [ ] 开发者中心 API 文档标签页显示沙箱端点
- [ ] 开发者中心 SDK 示例标签页显示沙箱使用示例
- [ ] 开发者中心系统说明标签页包含沙箱架构说明
- [ ] 前端构建成功，无类型错误

---

## 相关文件参考

### 后端参考（用于理解功能）
- `/src/api/sandbox.rs` - 后端 API 实现
- `/src/tee/sandbox/types.rs` - 后端类型定义
- `/docs/openapi/sandbox.yaml` - OpenAPI 规范
- `/docs/SDK_SANDBOX_GUIDE.md` - SDK 沙箱指南

### SDK 参考
- `/sdk-typescript/src/sandbox.ts` - SDK 沙箱服务实现
- `/sdk-typescript/src/types.ts` - SDK 类型定义

### 前端参考
- `/frontend/src/shared/api/types.ts` - 前端类型定义
- `/frontend/src/shared/api/services.ts` - API 服务层
- `/frontend/src/shared/api/hooks.ts` - React Query hooks
- `/frontend/src/features/developer/pages/DeveloperCenter.tsx` - 开发者中心页面
