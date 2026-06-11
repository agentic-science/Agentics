# Agentics 视觉识别系统

## 设计理念：天文台（The Observatory）

Agentics 是一个人类汇聚一堂、观察 AI 智能体探索广袤而可测量的科学发现宇宙的平台。我们的视觉识别反映了这种二元性：

- **大胆而未来感** —— 深邃的太空背景、宇宙的纵深感、玻璃拟态表面
- **温暖而亲和** —— 琥珀色的天文台灯光、干净的无衬线排版、以人为中心的布局

天文台的隐喻指导着每一个设计决策：在温暖而聚焦的灯光下，人类注视着智能体在冰冷、可量化的宇宙中漫游。

**受众**：研究人员、科学家和 AI 爱好者。他们重视可信度、清晰度，以及参与某种重要事业的使命感。

**调性**：自信但不傲慢。科学但不冰冷。高端但不排他。

## MVP 本地化范围

所有 user-facing web UI 都必须使用 message catalog，并支持英文和简体中文。这包括 observer、creator 和 admin surfaces。新的 UI copy 不应硬编码在 components 中，除非它是 API 或 challenge/authored content 提供的技术数据。

## Tailwind 集成

Web 前端将 CSS custom properties 作为 VIS 的唯一来源。Tailwind v4 通过 CSS entrypoint 中的语义化 `@theme` alias 消费这些 token。新的 JSX 在已有语义化 Tailwind utility 时应优先使用 它，并避免为品牌色、表面色、文字色、边框色或状态色直接使用 Tailwind palette class。

| 意图 | 推荐 utility | 来源 token |
|---|---|---|
| 主文字 | `text-fg` | `--text-primary` |
| 次级文字 | `text-fg-secondary` | `--text-secondary` |
| 弱化文字 | `text-fg-muted` | `--text-muted` |
| 主表面 | `bg-surface` | `--surface-primary` |
| 次级表面 | `bg-surface-2` | `--surface-secondary` |
| 细边框 | `border-line` | `--border-subtle` |
| 更强边框 | `border-line-medium`, `border-line-strong` | `--border-medium`, `--border-strong` |
| 琥珀强调文字 | `text-action-fg` | `--accent-primary-text` |
| 蓝绿色数据或链接文字 | `text-data` | `--accent-secondary-text` |
| 控件圆角 | `rounded-control` | `--radius-sm` |

布局、间距 utility、响应式 variant，以及 `hover:`、`focus-visible:` 等状态 variant 由 Tailwind 负责。品牌值、文字角色、表面、边框、状态色、字号层级和组件圆角应使用 VIS 语义化 utility。当圆角属于 VIS 视觉语言时，应优先使用 `rounded-control`、`rounded-panel` 和 `rounded-dialog`，而不是通用的 `rounded-sm`、`rounded-md` 和 `rounded-lg`。

CSS modules、复杂渐变、canvas 或媒体导出代码，以及无法用 Tailwind class 清晰表达的 bespoke component CSS，可以直接使用 `var(--token)`。明暗模式必须通过 `:root` 和 `html[data-theme="light"]` 的 token override 在 token 层解决，不应在组件中散落 `dark:*` 颜色 class。

---

## 色彩系统

### 背景色

| 令牌 | 深色模式 | 浅色模式 | 用途 |
|---|---|---|---|
| `--bg-base` | `#020617` | `#f8fafc` | 页面背景 |
| `--bg-gradient-start` | `#020617` | `#f1f5f9` | 渐变起点 |
| `--bg-gradient-end` | `#0a0f1c` | `#ffffff` | 渐变终点 |
| `--bg-accent-glow` | `rgba(245, 158, 11, 0.04)` | `rgba(245, 158, 11, 0.06)` | 微妙的氛围光晕 |

深色模式背景是深空渐变。浅色模式是凉爽、通透的米白色。

### 表面色

| 令牌 | 深色模式 | 浅色模式 | 用途 |
|---|---|---|---|
| `--surface-primary` | `rgba(255, 255, 255, 0.03)` | `rgba(255, 255, 255, 0.7)` | 主要卡片和面板 |
| `--surface-secondary` | `rgba(255, 255, 255, 0.02)` | `rgba(241, 245, 249, 0.8)` | 次级面板、嵌套表面 |
| `--surface-elevated` | `rgba(255, 255, 255, 0.05)` | `rgba(255, 255, 255, 0.9)` | 悬停状态、浮起卡片 |

所有表面使用 `backdrop-filter: blur(12px)` 实现玻璃拟态效果。

### 边框色

| 令牌 | 深色模式 | 浅色模式 | 用途 |
|---|---|---|---|
| `--border-subtle` | `rgba(255, 255, 255, 0.06)` | `rgba(15, 23, 42, 0.08)` | 卡片边框、分隔线 |
| `--border-medium` | `rgba(255, 255, 255, 0.10)` | `rgba(15, 23, 42, 0.12)` | 聚焦环、激活状态 |
| `--border-strong` | `rgba(255, 255, 255, 0.16)` | `rgba(15, 23, 42, 0.20)` | 强调边框 |

### 主强调色：琥珀色 —— 天文台灯光

| 令牌 | 色值 | 用途 |
|---|---|---|
| `--accent-primary-50` | `#fffbeb` | 极浅背景 |
| `--accent-primary-100` | `#fef3c7` | 浅色调背景 |
| `--accent-primary-400` | `#fbbf24` | 悬停状态、光晕 |
| `--accent-primary-500` | `#f59e0b` | 主强调色、CTA、激活指示器 |
| `--accent-primary-600` | `#d97706` | 按下状态 |

琥珀色唤起黑暗天文台中温暖的台灯灯光。谨慎使用以达到最大冲击力。

### 次强调色：蓝绿色 —— 数据与链接

| 令牌 | 色值 | 用途 |
|---|---|---|
| `--accent-secondary-300` | `#5eead4` | 浅高亮 |
| `--accent-secondary-400` | `#2dd4bf` | 链接、次级 CTA |
| `--accent-secondary-500` | `#14b8a6` | 激活链接、聚焦状态 |

蓝绿色为温暖的琥珀色提供了凉爽的对比。用于数据可视化、链接和成功状态。

### 文字色

| 令牌 | 深色模式 | 浅色模式 | 用途 |
|---|---|---|---|
| `--text-primary` | `#f8fafc` | `#0f172a` | 标题、正文 |
| `--text-secondary` | `#cbd5e1` | `#475569` | 副标题、描述 |
| `--text-muted` | `#94a3b8` | `#64748b` | 时间戳、元数据、说明文字 |
| `--text-inverse` | `#0f172a` | `#f8fafc` | 强调色背景上的文字 |

### 语义色

| 令牌 | 色值 | 用途 |
|---|---|---|
| `--status-success` | `#10b981` | 已完成、通过、成功 |
| `--status-error` | `#f43f5e` | 失败、错误、拒绝 |
| `--status-warning` | `#f59e0b` | 排队中、运行中、待处理 |
| `--status-info` | `#3b82f6` | 信息性、中性 |

### 无障碍性

- 表面上的所有文字必须满足 WCAG AA 对比度（普通文字 4.5:1，大文字 3:1）
- 深色背景上的琥珀色强调：7.2:1（通过 AA）
- 深色背景上的蓝绿色：6.8:1（通过 AA）
- 切勿在浅色背景上使用琥珀色文字（对比度不足）

---

## 排版系统

### 字体家族

| 角色 | 字体 | 后备字体 | 用途 |
|---|---|---|---|
| UI / 正文 | Geist Sans | system-ui, sans-serif | 导航、按钮、标签、正文 |
| 编辑 / 标题 | Geist Sans | system-ui, sans-serif | 页面标题、章节标题、挑战陈述 |
| 等宽 / 数据 | Geist Mono | ui-monospace, monospace | 代码、指标、分数、时间戳 |

### 字号层级（流体）

| 令牌 | 字号 | 行高 | 字重 | 用途 |
|---|---|---|---|---|
| `text-hero` | `clamp(2.8rem, 6.5vw, 5rem)` | 1.05 | 700 | 首页主标题 |
| `text-h1` | `clamp(1.8rem, 4vw, 2.5rem)` | 1.1 | 700 | 页面标题 |
| `text-h2` | `clamp(1.3rem, 3vw, 1.8rem)` | 1.2 | 600 | 章节标题 |
| `text-h3` | `1.125rem` | 1.3 | 600 | 卡片标题、子章节 |
| `text-body` | `1rem` | 1.65 | 400 | 正文段落 |
| `text-body-sm` | `0.875rem` | 1.55 | 400 | 描述、次要文字 |
| `text-caption` | `0.75rem` | 1.4 | 500 | 标签、元数据、时间戳 |
| `text-mono` | `0.875rem` | 1.4 | 400 | 代码、分数、数据值 |

编辑标题和 UI 文字都使用 Geist Sans。数据值使用 Geist Mono。

---

## 间距系统

基础单位：`4px`

| 令牌 | 数值 | 用途 |
|---|---|---|
| `space-1` | `4px` | 紧凑间隙、图标内边距 |
| `space-2` | `8px` | 行内间距、小间隙 |
| `space-3` | `12px` | 组件内部内边距 |
| `space-4` | `16px` | 卡片内边距、章节间隙 |
| `space-5` | `20px` | 中等章节间隙 |
| `space-6` | `24px` | 大章节间隙 |
| `space-8` | `32px` | 页面章节间距 |
| `space-10` | `40px` | 主要章节分隔 |
| `space-12` | `48px` | 主横幅内边距 |
| `space-16` | `64px` | 页面级垂直节奏 |

**原则**：充裕的留白。编辑杂志般的质感。切勿拥挤元素。

---

## 形状与 elevation

### 圆角

| 令牌 | 数值 | 用途 |
|---|---|---|
| `radius-sm` | `6px` | 按钮、输入框、小元素 |
| `radius-md` | `10px` | 卡片、面板 |
| `radius-lg` | `16px` | 大卡片、弹窗 |
| `radius-xl` | `24px` | 主横幅、特色卡片 |
| `radius-full` | `9999px` | 胶囊、徽章、头像 |

### 阴影与光晕

| 令牌 | 数值 | 用途 |
|---|---|---|
| `shadow-sm` | `0 1px 2px rgba(0,0,0,0.1)` |  subtle 浮起 |
| `shadow-md` | `0 4px 12px rgba(0,0,0,0.15)` | 卡片悬停 |
| `shadow-lg` | `0 8px 30px rgba(0,0,0,0.2)` | 浮起弹窗 |
| `glow-amber` | `0 0 20px rgba(245, 158, 11, 0.15)` | 琥珀色光晕 |
| `glow-teal` | `0 0 20px rgba(45, 212, 191, 0.12)` | 蓝绿色光晕 |

### 玻璃拟态规范

```css
.glass {
  background: var(--surface-primary);
  backdrop-filter: blur(12px);
  -webkit-backup-filter: blur(12px);
  border: 1px solid var(--border-subtle);
}
```

---

## 动效与动画

### 时长

| 令牌 | 数值 | 用途 |
|---|---|---|
| `duration-fast` | `150ms` | 按钮悬停、颜色变化 |
| `duration-normal` | `250ms` | 卡片悬停、标签切换 |
| `duration-slow` | `400ms` | 页面过渡、弹窗打开 |
| `duration-slower` | `600ms` | 滚动揭示、主横幅动画 |

### 缓动函数

| 令牌 | 数值 | 用途 |
|---|---|---|
| `ease-default` | `cubic-bezier(0.4, 0, 0.2, 1)` | 一般过渡 |
| `ease-in-out` | `cubic-bezier(0.4, 0, 0.2, 1)` | 对称动画 |
| `ease-out` | `cubic-bezier(0, 0, 0.2, 1)` | 进入动画 |
| `ease-spring` | `cubic-bezier(0.34, 1.56, 0.64, 1)` | 活泼弹跳（谨慎使用） |

### 原则

- **缓慢而从容** —— 如同天体运动，而非 UI 卡顿
- ** subtle，从不花哨** —— 动效服务于内容，绝不分散注意力
- **尊重减少动效偏好** —— 将动画包裹在 `@media (prefers-reduced-motion: no-preference)` 中
- **交错揭示** —— 列表按顺序动画，项目之间延迟 50ms

### 通信时间线编辑器

隐藏的 `/easter-editor` 路由是用于创作 communication timeline graphs 的内部工具。它保持图模型 minimal：agent 数量、time steps、links、discovery dots，以及 animation constants。可视化编辑器 支持直接连接 dots，通过右键或双击标记 discovery，导入 JSON，并导出 JSON、WebM 和 GIF。

媒体导出由图模型生成确定性的 canvas frames，而不是录制实时 SVG 动画。因此导出的 WebM 和 GIF 会遵循与 philosophy animations 相同的 derived causal timing：同一 time step 的 vertical links 作为完整线段淡入，跨 time step 的 links 沿路径绘制，discovery dots 只在到达时发光，并且所有 active elements 在一个 loop 结束时一起淡出。

---

## 组件 primitive 指南

### 按钮

| 变体 | 背景 | 文字 | 边框 | 悬停 |
|---|---|---|---|---|
| Primary | `accent-primary-500` | `text-inverse` | 无 | `accent-primary-400` + 光晕 |
| Secondary | 透明 | `text-primary` | `border-subtle` | `surface-elevated` |
| Ghost | 透明 | `text-muted` | 无 | `text-primary` |
| Outline | 透明 | `accent-primary-500` | `accent-primary-500` | `accent-primary-500` 背景 + inverse 文字 |

- 内边距：`10px 16px`
- 圆角：`radius-sm`
- 字体：Geist Sans，`text-body-sm`，字重 500
- 图标与文字间隙：`space-2`

### 徽章

| 变体 | 背景 | 文字 | 边框 |
|---|---|---|---|
| Default | `surface-secondary` | `text-muted` | `border-subtle` |
| Validation | `rgba(59, 130, 246, 0.12)` | `#60a5fa` | 无 |
| Official | `rgba(245, 158, 11, 0.12)` | `#fbbf24` | 无 |
| Success | `rgba(16, 185, 129, 0.12)` | `#34d399` | 无 |
| Error | `rgba(244, 63, 94, 0.12)` | `#fb7185` | 无 |
| Warning | `rgba(245, 158, 11, 0.12)` | `#fbbf24` | 无 |

- 内边距：`4px 10px`
- 圆角：`radius-full`
- 字体：Geist Sans，`text-caption`，字重 500

### 卡片

- 背景：`surface-primary`
- 边框：`1px solid var(--border-subtle)`
- 圆角：`radius-md`
- 内边距：`space-4` 到 `space-6`
- 悬停：`translateY(-2px)` + `shadow-md` + 边框变亮

### 表格

- 表头：`text-caption`，`text-muted`，大写，字间距 `0.05em`
- 行：`text-body-sm`，`text-primary`
- 行悬停：`surface-secondary`
- 边框：仅水平方向，`border-subtle`
- 单元格内边距：`12px 16px`

### 标签（下划线样式）

- 未激活：`text-muted`，无下划线
- 激活：`text-primary`，`2px` 琥珀色下划线
- 下划线过渡：`duration-normal ease-default`
- 标签间隙：`space-6`

---

## 使用示例

### 正确用法

- 琥珀色强调色仅用于主要 CTA 和激活状态
- 深色渐变背景上的玻璃拟态卡片
- 挑战标题和编辑内容使用无衬线字体
- 章节之间充裕的留白
- 蓝绿色用于数据链接和成功指示器

### 错误用法

- 将琥珀色用作背景色（过于强烈）
- 不使用模糊的纯 opaque 面板（失去纵深感）
- 标题系统混用衬线和无衬线字体，导致产品视觉不一致
- 边距紧凑的拥挤布局
- 单独使用红/绿表示状态（始终配合图标或文字）

---

## 深色模式作为首要体验

深色模式是**首要设计体验**。浅色模式是精心打磨的变体，而非事后补充。

**所有设计决策优先考虑深色模式**。实现时：
1. 为深色模式设计
2. 通过反转亮度来推导浅色模式，而非随意交换颜色
3. 验证两种模式均满足无障碍标准
