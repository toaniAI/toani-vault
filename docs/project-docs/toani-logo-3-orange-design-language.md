# Toani.ai LOGO 3.0 设计语言梳理（橘色版）

## 1. 文档目的

这份文档用于把 Figma 文件 `Toani.ai` 中 `LOGO 3.0` 的橘色方案，转成前端可执行的品牌语言说明。

目标不是重做整套 UI，而是回答 4 件事：

- LOGO 3.0 橘色版到底在表达什么
- 前端哪些地方应该跟着一起变，不只是替换一个图标
- 颜色、材质、排版、动效应该如何配合 LOGO
- 当前仓库里偏绿色的 Toani 主题，应该怎样迁移到橘色品牌语法

说明：

- 当前仓库已有一份偏绿色的品牌提炼文档：[toani-ui-style-guide-for-credbridge.md](/Users/yvan/AIWorkspace/credbridge/docs/project-docs/toani-ui-style-guide-for-credbridge.md)
- 本文只覆盖 `LOGO 3.0 橘色版` 的品牌语言及其对前端的影响
- 若后续设计导出提供了精确 token，本文件中的色值应以设计导出为最终准绳；但命名、层级和用法建议可以先执行

## 2. 一句话定义

`LOGO 3.0 橘色版` 的核心不是“把绿色改成橘色”，而是把品牌气质从 `冷静的安全控制台` 调整为 `有温度的智能代理入口`。

对应到前端语义，是这几个方向：

- 从 `安全、理性、克制` 转向 `智能、主动、亲和、可唤醒`
- 从 `植物感荧光绿` 转向 `日出感橘光`
- 从 `冷光材质` 转向 `暖光折射`
- 从 `系统工具感` 转向 `品牌入口感`

## 3. LOGO 3.0 的设计语言

### 3.1 品牌气质关键词

- `Warm Intelligence`
- `Agentic Energy`
- `Soft Futurism`
- `Signal over Decoration`

对应的主观感受：

- 有科技感，但不冷
- 有发光感，但不刺眼
- 有识别度，但不走互联网工具 Logo 的扁平通用路线

### 3.2 形态语言

虽然当前任务聚焦橘色版，但前端实现上应把 LOGO 理解成一组稳定几何规则，而不是一张图片。

建议按以下几条来理解：

- 轮廓是 `简化、闭合、可单色表达` 的图形
- 内部结构应有 `指向性` 或 `能量聚焦感`
- 视觉重心偏中上，适合放在导航、启动页和 Hero 左上角
- 圆角或曲线比尖锐折线更重要，体现亲和与流动
- 单色状态下必须仍可识别，不能依赖复杂渐变才能成立

### 3.3 色彩情绪

橘色版不是高饱和警示橙，而应更接近：

- `晨光橙`
- `琥珀橙`
- `落日金橙`

避免这两种错误方向：

- 过红：会接近警告/错误语义
- 过黄：会失去品牌稳定性，像活动页高亮色

### 3.4 材质语言

LOGO 3.0 橘色版更适合以下材质逻辑：

- 主体为实色或近实色，不依赖复杂纹理
- 允许非常轻的高光或暖色光晕
- 发光应该是 `边缘呼吸`，不是大范围霓虹外扩
- 在深色背景上呈现为 `被点亮`，而不是 `贴上去`

## 4. 对前端的直接含义

如果采用 LOGO 3.0 橘色版，前端不能只替换 `navbar logo`。以下区域都要一起调整。

### 4.1 必须同步调整的层

- 顶部导航品牌区
- 登录页 Hero 和品牌入口区
- 空状态插图/加载态/启动页
- 主 CTA 按钮
- 焦点态 glow
- 品牌渐变背景
- Badge/Tag 中的品牌强调态

### 4.2 不应该被橘色接管的层

橘色是品牌主强调，不应该吞掉全部语义色。

- 成功态仍应保留绿色语义
- 错误态仍应保留红色语义
- 警告态可以与橘色接近，但必须和品牌主色拉开层级
- 数据图表不能全部改成橘色，否则会损失可读性和区分度

## 5. 品牌配色建议

### 5.1 推荐 token 结构

前端先改命名体系，再改具体数值。不要继续沿用当前绿色语义里的 `lime` 心智。

建议新增或替换为：

- `--brand-primary`
- `--brand-primary-hover`
- `--brand-primary-active`
- `--brand-secondary`
- `--brand-highlight`
- `--brand-deep`
- `--brand-glow`
- `--brand-glow-strong`

### 5.2 推荐色板方向

以下色值用于前端改造的第一版落地，可作为设计未导出前的实施基线：

- `--brand-primary: #F28A2E`
- `--brand-primary-hover: #FF9C45`
- `--brand-primary-active: #D96F16`
- `--brand-secondary: #FFB36B`
- `--brand-highlight: #FFD6A6`
- `--brand-deep: #8F4712`
- `--brand-glow: rgba(242, 138, 46, 0.24)`
- `--brand-glow-strong: rgba(255, 182, 107, 0.34)`

这组颜色对应的关系是：

- `primary` 用于品牌按钮、主 Logo、焦点线索
- `secondary` 用于弱化强调、大面积柔光
- `highlight` 用于高光、边缘照明、Hero 柔和反射
- `deep` 用于深色混合、悬停压暗、深背景上的局部过渡

### 5.3 背景层建议

橘色版不适合配纯白和高亮浅灰大背景，建议继续保留深色舞台，但把冷绿调改成暖棕黑。

建议深色基底：

- `--bg-0: #140D09`
- `--bg-1: #1C120C`
- `--surface-0: #241710`
- `--surface-1: #2F1F15`
- `--surface-2: #3A2619`
- `--text-primary: #FFF3E7`
- `--text-secondary: #D9BBA2`
- `--stroke-soft: rgba(255, 214, 166, 0.16)`

### 5.4 渐变建议

- Hero 暖光：
  `linear-gradient(135deg, #140D09 0%, #2A180F 35%, #8F4712 72%, #FFD6A6 100%)`
- 品牌柔光：
  `radial-gradient(circle at 30% 30%, rgba(255, 182, 107, 0.34) 0%, rgba(242, 138, 46, 0.2) 36%, rgba(20, 13, 9, 0) 72%)`

## 6. 组件级指导

### 6.1 Logo 本体

- 默认优先使用单色 SVG，不维护多份烘焙色图片
- SVG 填色用 `currentColor`
- 深色背景默认色使用 `--brand-primary` 或 `--brand-highlight`
- 浅色背景默认色使用 `--brand-deep`
- 禁止在导航条上使用复杂多段渐变 Logo
- 禁止给 Logo 增加投影描边来“提高存在感”

### 6.2 Navbar 品牌区

- Logo 与字标间距建议 `8-12px`
- 品牌区 hover 仅提亮，不做缩放弹跳
- 品牌区外层可有轻微暖色光斑，但不能影响导航可读性

### 6.3 Primary Button

橘色版落地后，主按钮必须同步品牌化，否则页面会出现 `橘色 Logo + 绿色 CTA` 的冲突。

- 默认：`brand-primary`
- Hover：`brand-primary-hover`
- Active：`brand-primary-active`
- 文字颜色：深色字优先，避免白字压不住亮橙底
- 阴影：使用短半径暖色 glow，不要使用大面积模糊光球

### 6.4 Focus / Active 态

- Focus ring 建议从绿色 ring 改为暖橙光圈
- 表单 focus 不是错误提示，不要过红
- Selected card 可以使用 `1px warm stroke + soft glow`

### 6.5 Card / Panel

橘色品牌版不意味着卡片也要变橘。卡片应保持深色中性，只在边界或热点位置出现暖色提示。

- 卡片底色：深棕黑系
- 边框：低透明暖色描边
- Hover：轻提亮边框或顶部反光
- 禁止整张卡变成大面积橙色面板

## 7. 排版与文案气质

LOGO 3.0 橘色版更适合如下排版语气：

- 标题更像 `入口宣言`，不是后台系统名录
- 文案可稍微更短、更有动作感
- 品牌标题允许使用 `warm highlight word`，但只高亮 1-2 个词

建议：

- H1/H2 不超过两种字重
- 高亮词只用 `brand-primary` 或 `brand-highlight`
- 不要出现绿色高亮关键词

## 8. 动效原则

LOGO 3.0 橘色版应该是“被唤醒”的感觉，不是“系统已启动”的工业动画。

建议：

- 默认无持续旋转
- 允许 `180-260ms` 的轻微提亮或浮动
- 页面入场可做暖光渐入
- Hover 动效优先改亮度和光晕，避免大位移和强缩放

禁止：

- 弹跳
- 高频闪烁
- 霓虹灯式连续脉冲

## 9. 与当前仓库主题的迁移关系

当前前端实现里，品牌体系仍主要围绕绿色 token：

- [frontend/src/index.css](/Users/yvan/AIWorkspace/credbridge/frontend/src/index.css:52)
- [frontend/tailwind.config.js](/Users/yvan/AIWorkspace/credbridge/frontend/tailwind.config.js:11)

当前的核心问题不是实现错误，而是品牌主题仍是旧方向：

- `--brand-primary` 仍是绿色
- glow 仍是荧光绿语法
- 登录页与 Layout 背景仍是绿色漫射光
- 若直接换橘色 Logo，不改这些层，会出现视觉语言断裂

## 10. 前端改造清单

### 10.1 第一阶段：Token 迁移

- 把 `brand-primary / secondary / highlight / deep / glow` 全部迁到橘色系
- 清理命名中带 `lime` 心智的遗留表述
- 保留 `success / warning / error` 语义独立，不并入品牌色

### 10.2 第二阶段：品牌触点同步

- Navbar Logo 与字标
- 登录页品牌入口
- Hero 背景暖光
- 主 CTA 按钮
- 空态插图和 loading 点缀

### 10.3 第三阶段：弱化旧主题残留

- 去掉绿色 radial glow
- 去掉绿色品牌渐变
- 去掉绿色高亮文本
- 去掉绿色强调边框

## 11. 实现建议

### 11.1 SVG 组件接口

建议统一成：

```tsx
type BrandLogoProps = {
  size?: number;
  color?: string;
  glow?: boolean;
  className?: string;
};
```

实现原则：

- `color` 默认走 `currentColor`
- `glow` 只控制外围装饰层，不改变 Logo 主体几何
- 不把业务语义写死在 SVG 里

### 11.2 Token 映射建议

建议把品牌 token 暴露为：

```css
:root {
  --brand-primary: #F28A2E;
  --brand-primary-hover: #FF9C45;
  --brand-primary-active: #D96F16;
  --brand-secondary: #FFB36B;
  --brand-highlight: #FFD6A6;
  --brand-deep: #8F4712;
  --brand-glow: rgba(242, 138, 46, 0.24);
  --brand-glow-strong: rgba(255, 182, 107, 0.34);
}
```

然后再映射到 Tailwind 或 design token 层，不要在业务组件里到处写 hex。

## 12. 禁止事项

- 不要保留绿色主按钮再换橘色 Logo
- 不要把橘色用于所有状态色
- 不要把 Logo 做成高饱和渐变贴图
- 不要在浅色背景上继续使用浅橙 Logo
- 不要让品牌 glow 盖过正文内容

## 13. 验收标准

前端完成改造后，至少要满足以下检查项：

- 页面打开后，第一感知是 `暖色智能品牌`，不是 `绿色安全控制台`
- Logo、CTA、Hero glow 属于同一色彩家族
- Success/Error/Warning 仍能独立识别
- 深色背景下，Logo 有存在感但不刺眼
- 导航、登录页、空态、加载态的品牌语法统一

## 14. 建议的下一步

如果要把这份文档继续推进到实施层，建议按下面顺序做：

1. 先替换全局品牌 token
2. 再替换 Logo SVG 组件和 Navbar 品牌区
3. 再改登录页、Hero、空态和主按钮
4. 最后统一 glow、渐变、插图和品牌文案高亮

如果后续能从 Figma 节点 `160:2288` 导出精确色板和尺寸，这份文档应继续补 3 个附录：

- 附录 A：精确色值与对比度
- 附录 B：Logo 最小尺寸、留白、反白规则
- 附录 C：SVG 资源、组件命名与代码映射表