# CredBridge UI 设计规范

> 基于 zk.me (https://www.zk.me/) 网站设计风格分析

## 1. 设计概述

### 1.1 产品定位
CredBridge 是商用级 AI 身份与凭证桥接系统，UI 设计需传达：
- **安全性**：TEE 可信执行环境、密码学保护
- **专业性**：对标 zk.me 金融科技形象
- **科技感**：Web3 风格、零知识证明架构

### 1.2 设计原则 (zk.me 风格)
1. **青色系主调**：深青绿传达专业、科技和信任感
2. **渐变层次**：使用青绿色系渐变创造视觉深度
3. **玻璃态效果**：半透明模糊效果隐喻"隐私保护"
4. **大圆角造型**：20px+ 圆角创造亲和力
5. **品牌色阴影**：使用青色调阴影保持一致性

### 1.3 品牌调性总结
| 特征 | 描述 |
|------|------|
| 色彩 | 以青绿色系为主，搭配柔和渐变色 |
| 形状 | 大圆角和胶囊形状 |
| 质感 | 玻璃态 (Glassmorphism) 增加层次 |
| 交互 | 短暂过渡 (0.1s-0.2s) 创造轻快感 |

## 2. 色彩系统

### 2.1 主色调 (Primary Colors)
zk.me 使用深青色系作为品牌主色，传达专业、科技和信任感。

| 名称 | 色值 | RGB | 用途 |
|------|------|-----|------|
| Primary Dark | `#002E33` | rgb(0, 46, 51) | 主按钮、主标题 |
| Primary Base | `#005563` | rgb(0, 85, 99) | 正文、次要元素 |
| Primary Light | `#345D62` | rgb(52, 93, 98) | 强调文字 |

### 2.2 辅助色 (Secondary Colors - Teal Gradients)

| 名称 | 色值 | RGB | 用途 |
|------|------|-----|------|
| Teal 100 | `#DCFBE4` | rgb(220, 251, 244) | 最浅青绿背景 |
| Teal 200 | `#AFF5E4` | rgb(175, 245, 228) | 浅青绿渐变 |
| Teal 300 | `#57D2E5` | rgb(87, 210, 229) | 中青绿渐变 |
| Teal 400 | `#8EDDE3` | rgb(142, 221, 227) | 深青绿 |
| Teal 500 | `#B0F6FB` | rgb(176, 246, 251) | 亮青绿 |

### 2.3 强调色 (Accent Colors)

| 名称 | 色值 | RGB | 用途 |
|------|------|-----|------|
| Accent Mint | `#D9F6EF` | rgb(217, 246, 239) | 按钮文字 (深色背景上) |
| Accent Sky | `#E8FEFF` | rgb(232, 254, 255) | 浅色背景 |
| Accent Blue | `#64ABFF` | rgb(100, 171, 255) | 科技蓝链接 |
| Accent Coral | `#FFCCDE` | rgb(255, 204, 222) | 装饰元素 |
| Accent Lemon | `#FFE28E` | rgb(255, 226, 142) | 装饰元素 |

### 2.4 背景色层次 (Background Layers)

| 名称 | 色值 | 用途 |
|------|------|------|
| Body | `#9ABFBB` | rgb(154, 191, 187) - 页面整体背景 |
| Surface | `#FFFFFF` | 卡片/表面背景 |
| Subtle | `rgba(0, 85, 99, 0.08)` | 微妙背景色 |
| Overlay | `rgba(255, 255, 255, 0.24)` | 半透明覆盖层 |
| Footer | `rgba(0, 46, 51, 0.4)` | footer 半透明背景 |

### 2.5 文字颜色 (Text Colors)

| 名称 | 色值 | 用途 |
|------|------|------|
| Primary | `#002E33` | 主标题文字 |
| Secondary | `#005563` | 正文文字 |
| Tertiary | `#345D62` | 次要/辅助文字 |
| Muted | `#7B9395` | rgb(123, 147, 149) - 弱化文字 |
| Disabled | `#909399` | rgb(144, 147, 153) - 禁用文字 |
| Inverse | `#FFFFFF` | 深色背景上的文字 |

### 2.6 渐变色定义 (Gradient Definitions)

```css
/* Hero 区域渐变 */
--gradient-hero: linear-gradient(
  244.43deg,
  #DCFBE4 13.92%,
  #AFF5E4 28.61%,
  #57D2E5 88.16%
);

/* 深色 Section 渐变 */
--gradient-section-dark: linear-gradient(
  180deg,
  #005563 0%,
  #013F46 100%
);

/* 卡片渐变 - 青绿 */
--gradient-card: linear-gradient(
  186deg,
  #8EDDE3 4.25%,
  #E8FEFF 95.23%
);

/* 卡片渐变 - 蓝 */
--gradient-card-blue: linear-gradient(
  188deg,
  #97CDFF 9.52%,
  #EBF4FF 96.6%
);

/* 玻璃态渐变 */
--gradient-glass: linear-gradient(
  rgba(255, 255, 255, 0.7),
  rgba(255, 255, 255, 0.3)
);
```

### 2.7 Tailwind 配色配置

```js
// tailwind.config.js
module.exports = {
  theme: {
    extend: {
      colors: {
        primary: {
          dark: '#002E33',
          base: '#005563',
          light: '#345D62',
        },
        teal: {
          100: '#DCFBE4',
          200: '#AFF5E4',
          300: '#57D2E5',
          400: '#8EDDE3',
          500: '#B0F6FB',
        },
        accent: {
          mint: '#D9F6EF',
          sky: '#E8FEFF',
          blue: '#64ABFF',
          coral: '#FFCCDE',
          lemon: '#FFE28E',
        },
      },
    },
  },
}
```

## 3. 字体系统 (Typography)

### 3.1 字体家族 (Font Families)

```css
/* 主字体栈 - 优先使用 HarmonyOS Sans */
--font-sans: 'HarmonyOS_Sans', 'PingFang SC', 'Source Han Sans CN',
             'system-ui', 'Microsoft YaHei UI', 'Microsoft YaHei',
             'Helvetica Neue', Arial, sans-serif;
```

### 3.2 字号层级 (Type Scale)

| 层级 | 字号 | 字重 | 行高 | 用途 |
|------|------|------|------|------|
| Display | 60px | 700 | 1.2 | 超大展示标题 |
| H1 | 50px | 700 | 60px | 主标题 |
| H2 | 36px | 700 | 50.4px | 章节标题 |
| H3 | 30px | 700 | 1.3 | 子标题 |
| H4 | 26px | 700 | 1.3 | 小标题 |
| H5 | 24px | 700 | 1.4 | 小小标题 |
| H6 | 22px | 700 | 1.4 | 微小标题 |
| Body-L | 18px | 400/500 | 1.6 | 大正文 |
| Body | 16px | 400 | 1.6 | 标准正文 |
| Body-S | 14px | 400/500 | 30px | 小正文 |
| Caption | 13px | 400 | 1.5 | 说明文字 |
| Small | 12px | 400/500 | 1.5 | 辅助文字 |

### 3.3 字重 (Font Weights)

| 名称 | 值 | 用途 |
|------|-----|------|
| Light | 300 | 轻量文字 |
| Regular | 400 | 标准正文 |
| Medium | 500 | 按钮、强调 |
| Bold | 700 | 标题、重要文字 |

### 3.4 Tailwind Typography 配置

```js
// tailwind.config.js
module.exports = {
  theme: {
    extend: {
      fontSize: {
        'display': ['60px', { lineHeight: '1.2', fontWeight: '700' }],
        'h1': ['50px', { lineHeight: '60px', fontWeight: '700' }],
        'h2': ['36px', { lineHeight: '50.4px', fontWeight: '700' }],
        'h3': ['30px', { lineHeight: '1.3', fontWeight: '700' }],
        'h4': ['26px', { lineHeight: '1.3', fontWeight: '700' }],
        'h5': ['24px', { lineHeight: '1.4', fontWeight: '700' }],
        'h6': ['22px', { lineHeight: '1.4', fontWeight: '700' }],
        'body-lg': ['18px', { lineHeight: '1.6', fontWeight: '400' }],
        'body': ['16px', { lineHeight: '1.6', fontWeight: '400' }],
        'body-sm': ['14px', { lineHeight: '30px', fontWeight: '400' }],
        'caption': ['13px', { lineHeight: '1.5', fontWeight: '400' }],
        'small': ['12px', { lineHeight: '1.5', fontWeight: '400' }],
      },
      fontWeight: {
        light: '300',
        regular: '400',
        medium: '500',
        bold: '700',
      },
    },
  },
}
```

## 4. 间距系统 (Spacing)

### 4.1 基础间距 (基于 4px 单位)

| Token | 值 | 用途 |
|-------|-----|------|
| space-1 | 4px | 最小间距 |
| space-2 | 8px | 小组件间距 |
| space-3 | 12px | 紧凑间距 |
| space-4 | 16px | 标准组件内间距 |
| space-5 | 20px | 中等间距 |
| space-6 | 24px | 卡片内间距 |
| space-8 | 32px | 组件间距 |
| space-10 | 40px | 区块间距 |
| space-12 | 48px | 大间距 |
| space-16 | 64px | 超大间距 |
| space-20 | 80px | section padding |
| space-24 | 96px | 大 section |
| space-28 | 112px | 超大 section |
| space-32 | 128px | 最大 section |

### 4.2 容器宽度
- 最大宽度：`1400px`
- 页面内边距：`24px` (移动端) / `48px` (桌面端)
- 卡片内边距：`16px`

## 5. 圆角与阴影系统

### 5.1 圆角规范 (Border Radius)

| Token | 值 | 用途 |
|-------|-----|------|
| sm | 10px | 小元素 |
| md | 14px | 标准卡片 |
| lg | 16px | 大卡片 |
| xl | 20px | 按钮 |
| 2xl | 24px | 大组件 |
| 3xl | 28px | 超大组件 |
| pill | 37px | 输入框专用 |
| full | 9999px | 完全圆角/胶囊形 |

### 5.2 阴影规范 (Shadows)

```css
/* 阴影系统 - 使用品牌色 */
--shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.3);
--shadow-md: 0 4px 14px rgba(0, 0, 0, 0.05);
--shadow-lg: 0 20px 40px rgba(0, 85, 99, 0.2);
--shadow-xl: 0 50px 100px rgba(0, 85, 99, 0.4);

/* 特殊用途阴影 */
--shadow-card: 0 20px 40px rgba(0, 85, 99, 0.2);
--shadow-hover: 0 25px 50px rgba(0, 85, 99, 0.25);
--shadow-button: none;  /* 按钮默认无阴影 */
```

### 5.3 Tailwind 配置

```js
// tailwind.config.js
module.exports = {
  theme: {
    extend: {
      spacing: {
        '1': '4px', '2': '8px', '3': '12px', '4': '16px',
        '5': '20px', '6': '24px', '8': '32px', '10': '40px',
        '12': '48px', '16': '64px', '20': '80px', '24': '96px',
        '28': '112px', '32': '128px',
      },
      borderRadius: {
        'sm': '10px', 'md': '14px', 'lg': '16px',
        'xl': '20px', '2xl': '24px', '3xl': '28px',
        'pill': '37px', 'full': '9999px',
      },
      boxShadow: {
        'sm': '0 1px 3px rgba(0, 0, 0, 0.3)',
        'md': '0 4px 14px rgba(0, 0, 0, 0.05)',
        'lg': '0 20px 40px rgba(0, 85, 99, 0.2)',
        'xl': '0 50px 100px rgba(0, 85, 99, 0.4)',
        'card': '0 20px 40px rgba(0, 85, 99, 0.2)',
        'hover': '0 25px 50px rgba(0, 85, 99, 0.25)',
      },
    },
  },
}
```

## 6. 组件规范 (Component Styles)

### 6.1 按钮 (Button)

#### 主按钮 (Primary Button)
```css
.btn-primary {
  background-color: #002E33;
  color: #D9F6EF;
  border-radius: 20px;
  padding: 11px 20px;
  font-size: 14px;
  font-weight: 500;
  border: none;
  transition: 0.1s ease;
  cursor: pointer;
}

.btn-primary:hover {
  opacity: 0.9;
  transform: translateY(-1px);
}
```

#### 次按钮 (Secondary Button)
```css
.btn-secondary {
  background-color: #FFFFFF;
  color: #005563;
  border-radius: 20px;
  padding: 11px 20px;
  font-size: 14px;
  font-weight: 500;
  border: none;
  transition: 0.1s ease;
}

.btn-secondary:hover {
  background-color: rgba(255, 255, 255, 0.9);
  transform: translateY(-1px);
}
```

#### 小按钮 (Small Button)
```css
.btn-sm {
  background-color: #002E33;
  color: #D9F6EF;
  border-radius: 20px;
  padding: 6px 20px;
  font-size: 12px;
  font-weight: 500;
}
```

#### Tailwind 按钮组件
```jsx
// components/Button.jsx
export function Button({ variant = 'primary', size = 'md', children, ...props }) {
  const baseStyles = 'inline-flex items-center justify-center font-medium rounded-[20px] transition-all duration-100';

  const variants = {
    primary: 'bg-[#002E33] text-[#D9F6EF] hover:opacity-90 hover:-translate-y-0.5',
    secondary: 'bg-white text-[#005563] hover:bg-white/90 hover:-translate-y-0.5',
    ghost: 'bg-transparent text-[#005563] hover:bg-[#005563]/8',
  };

  const sizes = {
    sm: 'px-5 py-1.5 text-xs',
    md: 'px-5 py-2.75 text-sm',
    lg: 'px-6 py-3 text-base',
  };

  return (
    <button
      className={`${baseStyles} ${variants[variant]} ${sizes[size]}`}
      {...props}
    >
      {children}
    </button>
  );
}
```

### 6.2 卡片 (Card)

#### 标准卡片
```css
.card {
  background-color: rgba(255, 255, 255, 0.24);
  border-radius: 14px;
  padding: 16px;
  border: 1px solid rgba(0, 85, 99, 0.12);
  backdrop-filter: blur(10px);
}
```

#### 内容卡片
```css
.card-content {
  background-color: rgba(0, 85, 99, 0.08);
  border-radius: 14px;
  padding: 16px;
}
```

#### 渐变卡片
```css
.card-gradient {
  background: linear-gradient(186deg, #8EDDE3 4.25%, #E8FEFF 95.23%);
  border-radius: 20px;
  padding: 24px;
}
```

#### Tailwind 卡片组件
```jsx
// components/Card.jsx
export function Card({ variant = 'default', children, className = '' }) {
  const variants = {
    default: 'bg-white/24 border border-[#005563]/12 backdrop-blur-sm',
    content: 'bg-[#005563]/8',
    gradient: 'bg-gradient-to-b from-[#8EDDE3] via-[#8EDDE3] to-[#E8FEFF]',
    glass: 'bg-white/24 backdrop-blur-md border border-white/30',
  };

  return (
    <div className={`rounded-[14px] p-4 ${variants[variant]} ${className}`}>
      {children}
    </div>
  );
}
```

### 6.3 表单元素 (Form Elements)

#### 输入框
```css
.input {
  background-color: rgba(255, 255, 255, 0.8);
  border-radius: 37px;
  padding: 0 15px;
  color: #0B4B52;
  border: none;
  font-size: 14px;
  height: 44px;
  transition: 0.2s ease;
}

.input:focus {
  outline: none;
  background-color: #FFFFFF;
  box-shadow: 0 0 0 2px rgba(0, 85, 99, 0.2);
}

.input::placeholder {
  color: rgba(0, 85, 99, 0.6);
}
```

#### Tailwind 输入框组件
```jsx
// components/Input.jsx
export function Input({ placeholder, ...props }) {
  return (
    <input
      className="w-full h-11 px-4 rounded-[37px] bg-white/80 text-[#0B4B52]
                 placeholder-[#005563]/60 border-none outline-none
                 focus:bg-white focus:ring-2 focus:ring-[#005563]/20
                 transition-all duration-200"
      placeholder={placeholder}
      {...props}
    />
  );
}
```

### 6.4 导航组件 (Navigation)

#### 导航栏
```css
.navbar {
  height: 72px;
  background-color: transparent;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 24px;
}
```

#### 导航链接
```css
.nav-link {
  color: #005563;
  text-decoration: none;
  font-weight: 500;
  font-size: 14px;
  padding: 8px 16px;
  border-radius: 20px;
  transition: 0.2s ease;
}

.nav-link:hover {
  background-color: rgba(0, 85, 99, 0.08);
}
```

### 6.5 页脚 (Footer)
```css
.footer {
  background-color: rgba(0, 46, 51, 0.4);
  color: #002E33;
  padding: 48px 24px;
}

.footer-link {
  color: #005563;
  text-decoration: none;
  font-size: 14px;
}

.footer-link:hover {
  text-decoration: underline;
}
```

### 6.6 图标风格 (Icon Style)

- 使用线性图标 (Stroke icons)
- 图标描边宽度：1.5px - 2px
- 图标颜色：与文字颜色保持一致
- 圆角端点 (round cap)

```css
.icon {
  stroke: currentColor;
  stroke-width: 1.5px;
  stroke-linecap: round;
  stroke-linejoin: round;
  fill: none;
}
```

## 7. 动画与交互 (Animation & Interaction)

### 7.1 过渡效果 (Transitions)

```css
/* 标准过渡 */
--transition-fast: 0.1s ease;       /* 按钮等小组件 */
--transition-base: 0.2s ease;       /* 卡片、输入框 */
--transition-slow: 0.3s ease;       /* 页面过渡 */
```

### 7.2 Hover 状态

```css
/* 按钮 Hover */
.btn-primary:hover {
  opacity: 0.9;
  transform: translateY(-1px);
}

.btn-secondary:hover {
  background-color: rgba(255, 255, 255, 0.9);
  transform: translateY(-1px);
}

/* 卡片 Hover */
.card:hover {
  transform: translateY(-2px);
  box-shadow: 0 25px 50px rgba(0, 85, 99, 0.25);
}

/* 导航链接 Hover */
.nav-link:hover {
  background-color: rgba(0, 85, 99, 0.08);
}
```

### 7.3 Active 状态

```css
.btn-primary:active,
.btn-secondary:active {
  transform: translateY(0);
}

.input:focus {
  background-color: #FFFFFF;
  box-shadow: 0 0 0 2px rgba(0, 85, 99, 0.2);
}
```

### 7.4 动画曲线 (Animation Curves)

```css
/* 动画缓动曲线 */
--ease-out: cubic-bezier(0.215, 0.61, 0.355, 1);
--ease-in-out: cubic-bezier(0.645, 0.045, 0.355, 1);
--ease-out-back: cubic-bezier(0.34, 1.56, 0.64, 1);
```

### 7.5 关键帧动画

```css
/* 淡入向上 */
@keyframes fadeInUp {
  from {
    opacity: 0;
    transform: translateY(20px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* 脉冲效果 */
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.7; }
}

.animate-fade-in-up {
  animation: fadeInUp 0.5s var(--ease-out) forwards;
}

.animate-pulse {
  animation: pulse 2s ease-in-out infinite;
}
```

### 7.6 Tailwind 动画配置

```js
// tailwind.config.js
module.exports = {
  theme: {
    extend: {
      transitionDuration: {
        'fast': '100ms',
        'base': '200ms',
        'slow': '300ms',
      },
      transitionTimingFunction: {
        'out': 'cubic-bezier(0.215, 0.61, 0.355, 1)',
        'in-out': 'cubic-bezier(0.645, 0.045, 0.355, 1)',
        'back': 'cubic-bezier(0.34, 1.56, 0.64, 1)',
      },
      keyframes: {
        'fade-in-up': {
          '0%': { opacity: '0', transform: 'translateY(20px)' },
          '100%': { opacity: '1', transform: 'translateY(0)' },
        },
        'pulse': {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0.7' },
        },
      },
      animation: {
        'fade-in-up': 'fade-in-up 0.5s out forwards',
        'pulse': 'pulse 2s ease-in-out infinite',
      },
    },
  },
}
```

## 8. 布局规范 (Layout)

### 8.1 导航栏 (Navbar)
- 高度：`72px`
- 背景：透明
- Padding：`0 24px`

### 8.2 页脚 (Footer)
- 背景：`rgba(0, 46, 51, 0.4)`
- Padding：`48px 24px`

### 8.3 Section 间距
- 标准 Section：`padding: 85px 0 0`
- 大 Section：`padding: 100px 48px`
- Footer Section：`padding: 140px 0 0`

## 9. 响应式断点

| 断点 | 宽度 | 说明 |
|------|------|------|
| sm | 640px | 小屏手机 |
| md | 768px | 平板 |
| lg | 1024px | 小桌面 |
| xl | 1280px | 标准桌面 |
| 2xl | 1536px | 大桌面 |

## 10. 设计 Token 系统 (Design Tokens)

### 10.1 CSS 变量完整定义

```css
/* design-tokens.css */
:root {
  /* Colors - Primary */
  --color-primary-dark: #002E33;
  --color-primary-base: #005563;
  --color-primary-light: #345D62;

  /* Colors - Teal */
  --color-teal-100: #DCFBE4;
  --color-teal-200: #AFF5E4;
  --color-teal-300: #57D2E5;
  --color-teal-400: #8EDDE3;
  --color-teal-500: #B0F6FB;

  /* Colors - Accent */
  --color-accent-mint: #D9F6EF;
  --color-accent-sky: #E8FEFF;
  --color-accent-blue: #64ABFF;
  --color-accent-coral: #FFCCDE;
  --color-accent-lemon: #FFE28E;

  /* Colors - Text */
  --color-text-primary: #002E33;
  --color-text-secondary: #005563;
  --color-text-tertiary: #345D62;
  --color-text-muted: #7B9395;
  --color-text-disabled: #909399;
  --color-text-inverse: #FFFFFF;

  /* Colors - Background */
  --color-bg-body: #9ABFBB;
  --color-bg-surface: #FFFFFF;
  --color-bg-subtle: rgba(0, 85, 99, 0.08);
  --color-bg-overlay: rgba(255, 255, 255, 0.24);
  --color-bg-footer: rgba(0, 46, 51, 0.4);

  /* Typography */
  --font-sans: 'HarmonyOS_Sans', 'PingFang SC', 'Source Han Sans CN', system-ui, sans-serif;
  --font-weight-light: 300;
  --font-weight-regular: 400;
  --font-weight-medium: 500;
  --font-weight-bold: 700;

  /* Spacing */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;
  --space-6: 24px;
  --space-8: 32px;
  --space-10: 40px;
  --space-12: 48px;
  --space-16: 64px;
  --space-20: 80px;
  --space-24: 96px;
  --space-28: 112px;
  --space-32: 128px;

  /* Border Radius */
  --radius-sm: 10px;
  --radius-md: 14px;
  --radius-lg: 16px;
  --radius-xl: 20px;
  --radius-2xl: 24px;
  --radius-3xl: 28px;
  --radius-pill: 37px;
  --radius-full: 9999px;

  /* Box Shadow */
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.3);
  --shadow-md: 0 4px 14px rgba(0, 0, 0, 0.05);
  --shadow-lg: 0 20px 40px rgba(0, 85, 99, 0.2);
  --shadow-xl: 0 50px 100px rgba(0, 85, 99, 0.4);

  /* Transition */
  --transition-fast: 0.1s ease;
  --transition-base: 0.2s ease;
  --transition-slow: 0.3s ease;
}
```

---

## 11. 设计原则总结 (zk.me 风格)

### 品牌调性
| 特征 | 描述 |
|------|------|
| **专业可信** | 深青色系传达安全、可靠的金融科技形象 |
| **科技前沿** | 渐变和玻璃态效果体现 Web3 和零知识证明的技术特性 |
| **隐私保护** | 半透明元素和模糊效果隐喻"隐私保护"的核心理念 |

### 视觉特征
| 元素 | 规范 |
|------|------|
| **色彩** | 以青绿色系为主，搭配柔和的渐变色 |
| **形状** | 大圆角 (20px+) 和胶囊形状创造亲和力 |
| **质感** | 玻璃态 (Glassmorphism) 效果增加层次感 |
| **阴影** | 使用品牌色调的阴影，保持一致性 |

### 交互理念
| 原则 | 说明 |
|------|------|
| **轻盈** | 短暂的过渡时间 (0.1s-0.2s) 创造轻快感 |
| **克制** | 动画幅度适中，不喧宾夺主 |
| **一致** | 所有交互反馈遵循统一的时间曲线 |

---

## 12. 参考资源

### 分析来源
- 网站 URL: https://www.zk.me/
- 分析时间：2026-03-12

### 生成文件
- `zkme-full-page-screenshot.png` - 完整页面截图
- `zkme-accessibility-snapshot.md` - 可访问性快照

---

*文档版本：v2.0 (zk.me 风格)*
*更新日期：2026-03-12*
*分析来源：https://www.zk.me/*
