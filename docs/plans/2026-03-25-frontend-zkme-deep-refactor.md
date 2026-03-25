# CredBridge 前端深度重构为 ZKME 设计语言 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `/Users/yvan/.codex/skills/zkme-frontend-design/SKILL.md` before making any frontend design or styling changes in this plan. Use `superpowers:executing-plans` to implement this plan task-by-task.

**Goal:** 将 `/Users/yvan/AIWorkspace/credbridge/frontend` 从当前的深色 shadcn/cyber 风格，深度重构为基于 `/Users/yvan/AIWorkspace/zkme-web` 的品牌化前端设计语言，同时保留现有 React 19 + Tailwind + shadcn/ui 的可维护架构。

**Architecture:** 不直接迁移 zkme-web 的 Vue 组件，而是通过 `zkme-frontend-design` skill 从 zkme-web 提取设计 token、字体策略、图标资源、布局模式和代表性组件表达，再在 CredBridge 的 React/shadcn 体系中重建主题层、组件变体层、应用壳层和关键页面视觉层。源项目 `/Users/yvan/AIWorkspace/zkme-web` 视为只读设计源，不在本计划内修改。

**Tech Stack:** React 19, TypeScript, Vite, Tailwind CSS, shadcn/ui, Radix UI, Zustand, Markdown

**Execution Status (2026-03-25):**

- Root doc updates: completed
- Frontend subrepo branch: `codex/frontend-zkme-deep-refactor-20260325`
- First-round milestone: completed
- Second-round page diffusion (`Task 8`): completed

---

## 强制执行规则

1. 开始任何实现前，必须先使用 `/Users/yvan/.codex/skills/zkme-frontend-design/SKILL.md`。
2. 所有设计映射必须以 `/Users/yvan/AIWorkspace/zkme-web` 的绝对路径为依据，不允许凭记忆抽象描述后直接编码。
3. 不允许直接复制 zkme-web 的 Vue 页面组件到 CredBridge。
4. 优先迁移顺序固定为：token、字体与图标、组件变体、应用壳、关键页面、静态资源。
5. 对每一批改动，都要记录“zkme 源文件 -> credbridge 目标文件”的映射。
6. 允许适配 React/shadcn 实现，但不允许保留当前紫蓝霓虹主视觉作为默认品牌方向。

---

## 当前基线

### 目标前端

- `/Users/yvan/AIWorkspace/credbridge/frontend/src/index.css`
- `/Users/yvan/AIWorkspace/credbridge/frontend/tailwind.config.js`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/app/Layout.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/auth/pages/LoginPage.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/dashboard/pages/DashboardPage.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/button.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/button-variants.ts`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/card.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/badge.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/sidebar.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/stores/themeStore.ts`

### 设计源

- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/components/ui-design-preview/_base.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/main.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/base/_reset.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/layout/_common.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/views/_Uidesign.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/utils/_fonts.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/utils/_iconfont.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/components/ui-design-preview/PreviewContent.vue`
- `/Users/yvan/AIWorkspace/zkme-web/src/views/integration/Uidesign.vue`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/img`

---

## 目标状态

### 视觉系统

- CredBridge 默认主题切换为 zkme 风格的可信、品牌化、浅底为主或可控双主题体系。
- 主色、语义色、圆角、边框、文本层级、背景层级全部由新的设计 token 驱动。
- 字体栈从 Geist 主导切换为 zkme 对应字体策略，至少完成兼容落地方案。

### 组件系统

- `Button`、`Card`、`Badge`、`Input`、`Dialog`、`Sidebar`、`Table`、`Tabs` 具备 zkme 风格变体。
- 现有页面中的局部视觉 hardcode 大幅下降，主要通过 token 和变体消费样式。

### 页面系统

- 登录页、主布局壳、Dashboard 至少达到“第一眼就是 zkme 体系”的程度。
- 后续 Credentials、Audit、Tokens、DeveloperCenter 可以在同一视觉规则下继续扩展。

### 资产系统

- 需要引入的 logo、插图、字体、图标资源有明确来源和落地路径。
- 对无法直接复用的 iconfont 或资源加载方式，存在 React/Tailwind 下的替代实现。

---

## 范围界定

### 本计划包含

- 主题 token 全量重构
- 字体与静态资源迁移策略
- shadcn/ui 核心变体重构
- 应用壳和关键页面深度改版
- 设计源映射文档补充

### 本计划不包含

- 后端接口重构
- 路由结构重写
- 业务功能新增
- 将 CredBridge 整体改回 Vue
- 修改 `/Users/yvan/AIWorkspace/zkme-web` 源项目

---

## 实施阶段

### Task 1: 建立设计源映射与重构约束

**Files:**

- Create: `/Users/yvan/AIWorkspace/credbridge/docs/plans/2026-03-25-frontend-zkme-deep-refactor.md`
- Create: `/Users/yvan/AIWorkspace/credbridge/docs/plans/frontend-zkme-source-map.md`

**Step 1: 使用 `zkme-frontend-design` skill 导出设计源清单**

运行并读取：

```bash
python3 /Users/yvan/.codex/skills/zkme-frontend-design/scripts/export_design_manifest.py
```

**Step 2: 在 `frontend-zkme-source-map.md` 中建立映射表**

至少记录以下列：

- zkme 源绝对路径
- 设计职责
- CredBridge 目标文件
- 迁移方式
- 风险点

**Step 3: 明确本次深度重构的一级目标文件**

首批必须覆盖：

- `/Users/yvan/AIWorkspace/credbridge/frontend/src/index.css`
- `/Users/yvan/AIWorkspace/credbridge/frontend/tailwind.config.js`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/app/Layout.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/auth/pages/LoginPage.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/dashboard/pages/DashboardPage.tsx`

**Acceptance:**

- 存在一份可追溯的设计源映射
- 文档中明确要求使用 `zkme-frontend-design` skill
- 后续实现人员不需要自行再猜测设计来源

---

### Task 2: 重构全局主题 token 与 Tailwind 映射

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/index.css`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/tailwind.config.js`
- Optional: `/Users/yvan/AIWorkspace/credbridge/frontend/src/App.css`

**Step 1: 用 zkme 语义替换现有默认主视觉**

重点移除或降级当前默认方向：

- `--color-primary: #6366f1`
- `--color-cyan`
- `--color-purple`
- glow 类阴影作为主品牌表达

改为以 zkme 源 token 为基础建立新的变量层：

- 品牌主色
- 品牌浅色背景
- 文本层级
- 成功/警告/风险语义
- 边框和 surface 层级

**Step 2: 保留 Tailwind 使用方式，但重绑到新 token**

在 `/Users/yvan/AIWorkspace/credbridge/frontend/tailwind.config.js` 中保留 shadcn 的变量映射机制，但使其消费新的 CSS 变量命名和取值。

**Step 3: 清理 Vite 初始残留样式**

如果 `/Users/yvan/AIWorkspace/credbridge/frontend/src/App.css` 仍有默认 Vite 样式残留，则删除或停用。

**Acceptance:**

- 全局 token 不再表现为默认紫蓝 cyber 风格
- Tailwind 颜色别名仍可正常使用
- 未改页面也能立即获得明显的品牌方向变化

---

### Task 3: 重构字体、图标与静态资源接入策略

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/index.css`
- Create or Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/assets/brand/*`
- Optional: `/Users/yvan/AIWorkspace/credbridge/frontend/public/*`

**Step 1: 从 zkme 源中确认字体策略**

参考：

- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/utils/_fonts.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/fonts/CircularStd-fonts`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/fonts/HarmonyOS_Sans`

根据可行性选择：

- 直接引入字体文件
- 或保留系统兼容回退并先做风格近似

**Step 2: 明确图标策略**

zkme-web 使用 iconfont；CredBridge 目前使用 `lucide-react`。本次不要求强制切回 iconfont，但要明确：

- 哪些地方继续用 Lucide
- 哪些品牌性图形改为静态 SVG 或资源文件
- 哪些地方需要引入 logo/hero 图

**Step 3: 接入品牌资源**

优先评估并迁移：

- `/Users/yvan/AIWorkspace/zkme-web/src/assets/img/logo.png`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/img/logo1.png`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/img/popup`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/img/icons.svg`

**Acceptance:**

- 字体策略已落实到代码层
- 品牌 logo 和关键插图有明确落点
- 图标方案不会阻塞后续页面重构

---

### Task 4: 重构 shadcn 核心组件变体

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/button-variants.ts`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/button.tsx`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/card.tsx`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/badge-variants.ts`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/badge.tsx`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/input.tsx`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/dialog.tsx`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/sidebar.tsx`
- Optional: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/table.tsx`
- Optional: `/Users/yvan/AIWorkspace/credbridge/frontend/src/components/ui/tabs.tsx`

**Step 1: 建立 zkme 风格按钮体系**

目标变体至少包含：

- primary
- secondary
- ghost
- subtle
- destructive

视觉应参考 zkme 的胶囊按钮、较大圆角、轻交互反馈，而不是当前 glow CTA 风格。

**Step 2: 重构卡片和容器气质**

参考 zkme 的 surface、边框、内边距、圆角、浅背景层级，而不是当前厚重深色卡片。

**Step 3: 重构 Sidebar 与导航项状态**

当前 sidebar 明显偏 shadcn 默认。需要重构：

- 宽度与间距
- 激活态
- 分组标题
- 顶部品牌区
- footer 用户区

**Acceptance:**

- 关键 shadcn 组件一眼可辨认出已脱离默认 New York 风格
- 页面层不需要再靠大量临时 class 堆叠才能接近 zkme 风格

---

### Task 5: 重构应用壳与导航框架

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/app/Layout.tsx`
- Optional: `/Users/yvan/AIWorkspace/credbridge/frontend/src/shared/stores/themeStore.ts`

**Step 1: 重做左侧导航品牌头部**

参考 zkme 的品牌表达，替换当前渐变盾牌图标和深色 glow 头部。

**Step 2: 重做顶部 Header**

使面包屑、背景、边框、留白、触发器和状态信息与新的设计系统一致。

**Step 3: 重新定义主题切换策略**

`themeStore` 当前支持 `light | dark | system`。本次要决定：

- 默认主题是什么
- 是否保留双主题
- zkme token 如何映射到 light/dark

如果双主题成本过高，第一轮允许只把默认主题做对，再保留 dark 作为兼容路径。

**Acceptance:**

- 布局壳已具备 zkme 体系的品牌辨识度
- 主题策略与 token 系统一致
- 导航、头部、内容区不再存在混合审美

---

### Task 6: 深度重构登录页

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/auth/pages/LoginPage.tsx`

**Step 1: 替换页面背景与视觉重心**

当前登录页是典型 cyber 安全风格。需要按 zkme 方向重做：

- 背景层次
- 中心卡片
- 品牌 logo 区
- 表单间距和文案层级
- 底部补充信息

**Step 2: 保留业务逻辑，仅重写视觉层**

不得改动以下业务核心：

- 输入验证
- 登录 mutation
- token 存储
- 错误处理

**Step 3: 减少纯装饰性 glow 和网格噪声**

避免继续使用大面积紫蓝光晕和赛博网格作为主视觉。

**Acceptance:**

- 登录页成为本轮设计语言迁移的样板页
- 功能行为不变，视觉完全换代

---

### Task 7: 深度重构 Dashboard

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/dashboard/pages/DashboardPage.tsx`

**Step 1: 重构页面标题区和统计卡片**

让标题、描述、指标卡、状态卡进入统一的 zkme 风格层次体系。

**Step 2: 重构活动流和右侧状态卡**

尤其处理：

- Card 头部
- Badge 变体
- 快捷入口
- 系统状态列表
- TEE 状态展示

**Step 3: 统一链接与交互细节**

例如“查看全部”、快捷入口 hover、图标容器等。

**Acceptance:**

- Dashboard 不再依赖大量 `primary/cyan/purple` 对比构建层次
- 数据可读性提升
- 品牌感与控制台效率平衡

---

### Task 8: 扩展重构到列表和业务页骨架

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/credentials/pages/CredentialsPage.tsx`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/audit/pages/AuditPage.tsx`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/tokens/pages/TokensPage.tsx`
- Modify: `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/developer/pages/DeveloperCenter.tsx`
- Optional: `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/tenants/pages/SettingsPage.tsx`
- Optional: `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/tenants/pages/UsersPage.tsx`

**Step 1: 先统一页面框架**

统一：

- 页面头部
- 过滤区
- 操作条
- 表格容器
- 空状态
- 弹窗和抽屉

**Step 2: 再处理每个页面的品牌细节**

避免每个页面自己发明颜色、卡片、标签和强调态。

**Acceptance:**

- 核心业务页进入同一视觉体系
- 组件复用明显增加

---

### Task 9: 验证、回归与文档收口

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/docs/project-docs/frontend-architecture.md`
- Optional: `/Users/yvan/AIWorkspace/credbridge/docs/UI_DESIGN_SPEC.md`
- All modified frontend files

**Step 1: 更新架构文档**

在前端架构文档中补充：

- 当前主题系统
- 设计源来源
- 组件变体策略
- 静态资源接入方式

**Step 2: 跑类型检查和构建**

运行：

```bash
cd /Users/yvan/AIWorkspace/credbridge/frontend
npx tsc --noEmit
npm run build
```

**Step 3: 手工验收关键页面**

至少检查：

- `/login`
- `/dashboard`
- `/credentials`
- `/audit`
- `/tokens`

**Acceptance:**

- 类型检查通过
- 构建通过
- 核心页面无明显布局破损
- 文档中明确说明使用了 `zkme-frontend-design` skill

---

## 第一轮实施顺序

1. Task 1
2. Task 2
3. Task 3
4. Task 4
5. Task 5
6. Task 6
7. Task 7
8. Task 9

说明：Task 8 作为第二轮扩散，不应阻塞第一轮把品牌系统、应用壳、登录页和 Dashboard 做到位。

## 当前任务进度

- [x] Task 1: 建立设计源映射与重构约束
- [x] Task 2: 重构全局主题 token 与 Tailwind 映射
- [x] Task 3: 重构字体、图标与静态资源接入策略
- [x] Task 4: 重构 shadcn 核心组件变体
- [x] Task 5: 重构应用壳与导航框架
- [x] Task 6: 深度重构登录页
- [x] Task 7: 深度重构 Dashboard
- [x] Task 8: 扩展重构到列表和业务页骨架
- [x] Task 9: 验证、回归与文档收口

---

## 验证清单

- [x] 已明确使用 `/Users/yvan/.codex/skills/zkme-frontend-design/SKILL.md`
- [x] 已建立 zkme 设计源到 CredBridge 文件的映射
- [x] `src/index.css` 已切换到 zkme 设计方向
- [x] `tailwind.config.js` 已与新 token 对齐
- [x] 字体和品牌资源方案已落地或明确降级策略
- [x] 核心 shadcn 组件变体已重构
- [x] `Layout.tsx` 已重构为 zkme 风格应用壳
- [x] `LoginPage.tsx` 已深度改版
- [x] `DashboardPage.tsx` 已深度改版
- [x] `CredentialsPage.tsx`、`AuditPage.tsx`、`TokensPage.tsx`、`DeveloperCenter.tsx` 已完成第二轮扩散
- [x] `SettingsPage.tsx`、`UsersPage.tsx` 已完成 zkme 骨架对齐
- [x] 页面级共享骨架 `frontend/src/components/zkme/page-shell.tsx` 已落地
- [x] 类型检查通过
- [x] 构建通过
- [x] 前端 lint 通过

## 子仓库提交信息

- Frontend subrepo path: `/Users/yvan/AIWorkspace/credbridge/frontend`
- Frontend subrepo branch: `codex/frontend-zkme-deep-refactor-20260325`
- Commit scope: full zkme design-system migration including second-round page diffusion

---

## 风险与应对

### 风险 1: Vue 设计源与 React 实现错位

应对：只迁移设计 token、布局原则、资源和组件表达，不迁移 Vue 实现细节。

### 风险 2: 字体与 iconfont 接入成本高

应对：先完成视觉兼容，再决定是否完整迁移字体与 iconfont。

### 风险 3: 控制台场景可用性被品牌化过度稀释

应对：以 Dashboard、表格页的可读性为约束，不盲目复刻营销站式装饰。

### 风险 4: 深度重构范围过大导致反馈回路变慢

应对：先完成“主题层 + 应用壳 + 登录页 + Dashboard”的第一轮里程碑，再扩散到业务页。

---

## 相关参考

### CredBridge

- `/Users/yvan/AIWorkspace/credbridge/frontend/src/index.css`
- `/Users/yvan/AIWorkspace/credbridge/frontend/tailwind.config.js`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/app/Layout.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/auth/pages/LoginPage.tsx`
- `/Users/yvan/AIWorkspace/credbridge/frontend/src/features/dashboard/pages/DashboardPage.tsx`
- `/Users/yvan/AIWorkspace/credbridge/docs/project-docs/frontend-architecture.md`

### ZKME 设计源

- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/components/ui-design-preview/_base.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/utils/_fonts.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/utils/_iconfont.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/assets/css/views/_Uidesign.scss`
- `/Users/yvan/AIWorkspace/zkme-web/src/components/ui-design-preview/PreviewContent.vue`
- `/Users/yvan/AIWorkspace/zkme-web/src/views/integration/Uidesign.vue`
- `/Users/yvan/.codex/skills/zkme-frontend-design/SKILL.md`
