---
title: 'Shadcn UI 前端框架重写'
slug: 'shadcn-ui-migration'
created: '2026-03-12'
status: 'completed'
stepsCompleted: [1, 2, 3, 4, 5]
tech_stack:
  - shadcn/ui
  - Radix UI
  - TailwindCSS 3.4.1
  - React 19.2.0
  - TypeScript 5.9.3
files_to_modify:
  - frontend/components.json
  - frontend/src/components/ui/*
  - frontend/src/app/Layout.tsx
  - frontend/src/features/**/*.tsx
code_patterns:
  - Shadcn 组件模式
  - TailwindCSS 工具类
  - class-variance-authority 变体
test_patterns: []
---

# Tech-Spec: Shadcn UI 前端框架重写

**Created:** 2026-03-12

## Overview

### Problem Statement

当前 CredBridge 前端 UI 组件实现存在以下问题：
1. 组件来源混乱 - 混用 `@base-ui/react` 和自定义 Shadcn 组件
2. components.json 配置不完整，未正确初始化 Shadcn UI
3. 缺失关键 Shadcn 组件（sidebar、breadcrumb、toast、skeleton、progress 等）
4. Layout 组件使用自定义侧边栏，未使用 Shadcn 官方 Sidebar 组件
5. 主题系统未与 Shadcn 标准变量对齐

### Solution

完全替换为官方 Shadcn UI 组件库，使用 `base-new-york` + `mauve` 中性色系统，重构所有页面和布局组件以使用标准化的 Shadcn 组件 API。

### Scope

**In Scope:**
- 重新初始化 Shadcn UI 配置（base-new-york 风格，mauve 中性色）
- 替换所有现有 UI 组件为官方 Shadcn 组件实现
- 添加缺失的 Shadcn 组件（sidebar、breadcrumb、toast、skeleton、progress、avatar、dropdown-menu、command 等）
- 重构 Layout 组件使用 Shadcn Sidebar
- 重构所有页面组件使用标准组件 API
- 更新主题系统以匹配 Shadcn 标准 CSS 变量
- 更新工具函数以使用 `cn()` 替代自定义 `utils.ts`

**Out of Scope:**
- 业务逻辑变更
- API 接口修改
- 状态管理架构变更（Zustand）
- 路由结构变更

## Context for Development

### Codebase Patterns

**当前项目结构:**
```
frontend/
├── src/
│   ├── app/
│   │   ├── App.tsx          # 应用根组件
│   │   ├── Layout.tsx       # 主布局（需要重构）
│   │   ├── router.tsx       # 路由配置
│   │   └── providers.tsx    # Provider 组件
│   ├── components/
│   │   └── ui/              # UI 组件目录（需要替换）
│   │       ├── button.tsx
│   │       ├── card.tsx
│   │       ├── dialog.tsx
│   │       ├── input.tsx
│   │       ├── label.tsx
│   │       ├── badge.tsx
│   │       ├── table.tsx
│   │       ├── textarea.tsx
│   │       ├── select.tsx
│   │       ├── alert-dialog.tsx
│   │       └── dropdown-menu.tsx
│   ├── features/
│   │   ├── dashboard/
│   │   ├── credentials/
│   │   ├── tokens/
│   │   ├── audit/
│   │   ├── auth/
│   │   └── tenants/
│   ├── shared/
│   │   ├── api/             # API 服务
│   │   ├── stores/          # Zustand stores
│   │   └── lib/             # 工具函数
│   └── lib/
│       └── utils.ts         # 工具函数（cn）
├── components.json          # Shadcn 配置
├── tailwind.config.js       # Tailwind 配置
└── package.json
```

**当前组件问题分析:**

| 组件 | 当前实现 | 问题 |
|------|----------|------|
| Button | `@base-ui/react/button` | 非 Shadcn 官方实现 |
| Card | 自定义实现 | 需对齐 Shadcn API |
| Dialog | 自定义实现 | 需使用 Radix UI Dialog |
| Input | 自定义实现 | 需对齐 Shadcn API |
| Sidebar | 完全自定义（Layout.tsx 内联） | 需使用 Shadcn Sidebar |

### Files to Reference

| File | Purpose |
| ---- | ------- |
| `frontend/components.json` | Shadcn UI 配置（需要更新为 base-new-york + mauve） |
| `frontend/tailwind.config.js` | TailwindCSS 主题配置（需要更新颜色变量） |
| `frontend/src/index.css` | 全局样式和 CSS 变量（需要更新为 Shadcn 标准） |
| `frontend/src/lib/utils.ts` | cn() 工具函数（已有，无需修改） |
| `frontend/src/app/Layout.tsx` | 主布局组件（需要使用 Shadcn Sidebar 重构） |
| `frontend/src/components/ui/button.tsx` | Button 组件（需要替换为 Shadcn 官方实现） |
| `frontend/src/components/ui/card.tsx` | Card 组件（需要替换为 Shadcn 官方实现） |
| `frontend/src/components/ui/dialog.tsx` | Dialog 组件（需要使用 Radix UI 重构） |
| `frontend/src/components/ui/input.tsx` | Input 组件（需要替换为 Shadcn 官方实现） |
| `frontend/src/components/ui/label.tsx` | Label 组件（需要替换为 Shadcn 官方实现） |
| `frontend/src/components/ui/badge.tsx` | Badge 组件（需要替换为 Shadcn 官方实现） |
| `frontend/src/components/ui/table.tsx` | Table 组件（需要替换为 Shadcn 官方实现） |
| `frontend/src/components/ui/textarea.tsx` | Textarea 组件（需要替换为 Shadcn 官方实现） |
| `frontend/src/components/ui/select.tsx` | Select 组件（需要使用 Radix UI 重构） |
| `frontend/src/components/ui/alert-dialog.tsx` | AlertDialog 组件（需要使用 Radix UI 重构） |
| `frontend/src/components/ui/dropdown-menu.tsx` | DropdownMenu 组件（需要使用 Radix UI 重构） |

### Technical Decisions

1. **Shadcn 风格**: `base-new-york` - 提供更现代的圆角和间距
2. **中性色**: `mauve` - 与当前深色主题兼容的中性灰色系
3. **图标库**: `lucide-react` - 已安装，保持使用
4. **CSS 变量**: 使用 Shadcn 标准 `--radius`、`--background`、`--foreground` 等
5. **组件导入**: 使用 `@/components/ui/xxx` 路径别名

## Implementation Plan

### Tasks

#### 阶段 1: 初始化 Shadcn UI 配置

1.1 **更新 components.json**
   - 路径：`frontend/components.json`
   - 修改 style 为 `new-york`
   - 修改 baseColor 为 `mauve`
   - 确保 aliases 正确配置

1.2 **更新 Tailwind 配置**
   - 路径：`frontend/tailwind.config.js`
   - 移除硬编码颜色值
   - 使用 CSS 变量引用颜色
   - 配置 Shadcn 标准颜色映射

1.3 **更新全局样式**
   - 路径：`frontend/src/index.css`
   - 添加 Shadcn 标准 CSS 变量
   - 保留现有自定义动画和工具类
   - 确保深色主题兼容

#### 阶段 2: 安装核心 Shadcn 组件

2.1 **运行 Shadcn CLI 安装组件**
```bash
cd frontend
npx shadcn@latest init --style new-york --base-color mauve
npx shadcn@latest add button
npx shadcn@latest add card
npx shadcn@latest add dialog
npx shadcn@latest add input
npx shadcn@latest add label
npx shadcn@latest add badge
npx shadcn@latest add table
npx shadcn@latest add textarea
npx shadcn@latest add select
npx shadcn@latest add alert-dialog
npx shadcn@latest add dropdown-menu
```

2.2 **安装扩展组件**
```bash
npx shadcn@latest add sidebar
npx shadcn@latest add breadcrumb
npx shadcn@latest add toast
npx shadcn@latest add skeleton
npx shadcn@latest add progress
npx shadcn@latest add avatar
npx shadcn@latest add command
npx shadcn@latest add separator
npx shadcn@latest add sheet
npx shadcn@latest add tooltip
npx shadcn@latest add scroll-area
npx shadcn@latest add collapsible
```

#### 阶段 3: 重构 Layout 组件

3.1 **更新 Layout.tsx**
   - 路径：`frontend/src/app/Layout.tsx`
   - 使用 Shadcn Sidebar 组件
   - 使用 Shadcn Breadcrumb 组件
   - 使用 Shadcn Avatar 组件
   - 使用 Shadcn DropdownMenu 组件
   - 保持现有功能和响应式行为

#### 阶段 4: 重构页面组件

4.1 **DashboardPage.tsx**
   - 路径：`frontend/src/features/dashboard/pages/DashboardPage.tsx`
   - 使用 Shadcn Card 组件
   - 使用 Shadcn Badge 组件
   - 使用 Shadcn Skeleton 组件（加载状态）

4.2 **CredentialsPage.tsx**
   - 路径：`frontend/src/features/credentials/pages/CredentialsPage.tsx`
   - 使用 Shadcn Dialog 组件
   - 使用 Shadcn AlertDialog 组件
   - 使用 Shadcn Input 组件
   - 使用 Shadcn Label 组件
   - 使用 Shadcn Textarea 组件
   - 使用 Shadcn Select 组件

4.3 **其他页面组件**
   - TokensPage.tsx
   - AuditPage.tsx
   - SettingsPage.tsx
   - UsersPage.tsx
   - ProfilePage.tsx
   - LoginPage.tsx
   - NotFoundPage.tsx

#### 阶段 5: 验证与测试

5.1 **类型检查**
```bash
cd frontend
npm run type-check
```

5.2 **Lint 检查**
```bash
npm run lint
```

5.3 **构建测试**
```bash
npm run build
```

5.4 **视觉验证**
- 验证所有页面渲染正确
- 验证深色主题正确应用
- 验证响应式布局正确

### Acceptance Criteria

**Given** 当前 CredBridge 前端代码库
**When** Shadcn UI 重写完成
**Then** 满足以下标准:

1. **组件完整性**
   - [ ] 所有 UI 组件使用 Shadcn 官方实现
   - [ ] components.json 配置正确（base-new-york + mauve）
   - [ ] CSS 变量与 Shadcn 标准对齐

2. **功能等价性**
   - [ ] 所有现有功能正常工作
   - [ ] 视觉样式与当前设计保持一致（深色主题）
   - [ ] 响应式布局正常工作

3. **代码质量**
   - [ ] 无 TypeScript 类型错误
   - [ ] 无 ESLint 错误
   - [ ] 组件遵循 Shadcn 最佳实践

## Additional Context

### Dependencies

**需要安装的依赖:**
```bash
cd frontend
npx shadcn@latest init --style new-york --base-color mauve
npx shadcn@latest add button
npx shadcn@latest add card
npx shadcn@latest add dialog
npx shadcn@latest add input
npx shadcn@latest add label
npx shadcn@latest add badge
npx shadcn@latest add table
npx shadcn@latest add textarea
npx shadcn@latest add select
npx shadcn@latest add alert-dialog
npx shadcn@latest add dropdown-menu
npx shadcn@latest add sidebar
npx shadcn@latest add breadcrumb
npx shadcn@latest add toast
npx shadcn@latest add skeleton
npx shadcn@latest add progress
npx shadcn@latest add avatar
npx shadcn@latest add command
npx shadcn@latest add separator
npx shadcn@latest add sheet
npx shadcn@latest add tooltip
```

### Testing Strategy

1. **视觉测试**: 对比重构前后的页面截图
2. **功能测试**: 验证所有交互功能
3. **类型检查**: `tsc --noEmit`
4. **Lint 检查**: `npm run lint`

### Notes

- 当前项目使用深色主题，Shadcn 组件需要适配深色模式
- 保持现有的动画和过渡效果
- 保持现有的自定义工具类（如 `.glow-primary`、`.glass` 等）
