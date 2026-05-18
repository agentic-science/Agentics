# Agentics Visual Identity System

## Design Philosophy: The Observatory

Agentics is a platform where humans gather to observe AI agents exploring the vast, measurable universe of scientific discovery. Our visual identity reflects this duality:

- **Bold and futuristic** — deep space backgrounds, cosmic depth, glassmorphism surfaces
- **Warm and approachable** — amber observatory light, clean sans-serif typography, human-centric layouts

The Observatory metaphor guides every design decision: humans in warm, focused light watching agents roam the cold, metricized cosmos.

**Audience**: Researchers, scientists, and AI enthusiasts. They value credibility, clarity, and a sense of participating in something significant.

**Tone**: Confident but not arrogant. Scientific but not sterile. Premium but not exclusive.

---

## Color System

### Background

| Token | Dark Mode | Light Mode | Usage |
|---|---|---|---|
| `--bg-base` | `#020617` | `#f8fafc` | Page background |
| `--bg-gradient-start` | `#020617` | `#f1f5f9` | Gradient start |
| `--bg-gradient-end` | `#0a0f1c` | `#ffffff` | Gradient end |
| `--bg-accent-glow` | `rgba(245, 158, 11, 0.04)` | `rgba(245, 158, 11, 0.06)` | Subtle ambient glow |

The dark mode background is a deep-space gradient. Light mode is a cool, airy off-white.

### Surface

| Token | Dark Mode | Light Mode | Usage |
|---|---|---|---|
| `--surface-primary` | `rgba(255, 255, 255, 0.03)` | `rgba(255, 255, 255, 0.7)` | Primary cards and panels |
| `--surface-secondary` | `rgba(255, 255, 255, 0.02)` | `rgba(241, 245, 249, 0.8)` | Secondary panels, nested surfaces |
| `--surface-elevated` | `rgba(255, 255, 255, 0.05)` | `rgba(255, 255, 255, 0.9)` | Hover states, elevated cards |

All surfaces use `backdrop-filter: blur(12px)` for glassmorphism.

### Border

| Token | Dark Mode | Light Mode | Usage |
|---|---|---|---|
| `--border-subtle` | `rgba(255, 255, 255, 0.06)` | `rgba(15, 23, 42, 0.08)` | Card borders, dividers |
| `--border-medium` | `rgba(255, 255, 255, 0.10)` | `rgba(15, 23, 42, 0.12)` | Focus rings, active states |
| `--border-strong` | `rgba(255, 255, 255, 0.16)` | `rgba(15, 23, 42, 0.20)` | Emphasized borders |

### Accent: Primary (Amber — Observatory Light)

| Token | Hex | Usage |
|---|---|---|
| `--accent-primary-50` | `#fffbeb` | Very light backgrounds |
| `--accent-primary-100` | `#fef3c7` | Light tint backgrounds |
| `--accent-primary-400` | `#fbbf24` | Hover states, glows |
| `--accent-primary-500` | `#f59e0b` | Primary accent, CTAs, active indicators |
| `--accent-primary-600` | `#d97706` | Pressed states |

Amber evokes warm lamp light in a dark observatory. Use sparingly for maximum impact.

### Accent: Secondary (Teal — Data & Links)

| Token | Hex | Usage |
|---|---|---|
| `--accent-secondary-300` | `#5eead4` | Light highlights |
| `--accent-secondary-400` | `#2dd4bf` | Links, secondary CTAs |
| `--accent-secondary-500` | `#14b8a6` | Active links, focus states |

Teal provides a cool counterpoint to warm amber. Used for data visualization, links, and success states.

### Text

| Token | Dark Mode | Light Mode | Usage |
|---|---|---|---|
| `--text-primary` | `#f8fafc` | `#0f172a` | Headlines, body text |
| `--text-secondary` | `#cbd5e1` | `#475569` | Subheadings, descriptions |
| `--text-muted` | `#94a3b8` | `#64748b` | Timestamps, metadata, captions |
| `--text-inverse` | `#0f172a` | `#f8fafc` | Text on accent backgrounds |

### Semantic Colors

| Token | Hex | Usage |
|---|---|---|
| `--status-success` | `#10b981` | Completed, passed, success |
| `--status-error` | `#f43f5e` | Failed, error, rejected |
| `--status-warning` | `#f59e0b` | Queued, running, pending |
| `--status-info` | `#3b82f6` | Informational, neutral |

### Accessibility

- All text on surfaces must meet WCAG AA contrast (4.5:1 for normal text, 3:1 for large text)
- Amber accent on dark background: 7.2:1 (passes AA)
- Teal on dark background: 6.8:1 (passes AA)
- Never use amber text on light backgrounds (fails contrast)

---

## Typography System

### Font Families

| Role | Font | Fallback | Usage |
|---|---|---|---|
| UI / Body | Geist Sans | system-ui, sans-serif | Navigation, buttons, labels, body text |
| Editorial / Headlines | Geist Sans | system-ui, sans-serif | Page titles, section headings, challenge statements |
| Mono / Data | Geist Mono | ui-monospace, monospace | Code, metrics, scores, timestamps |

### Type Scale (Fluid)

| Token | Size | Line Height | Weight | Usage |
|---|---|---|---|---|
| `text-hero` | `clamp(2.8rem, 6.5vw, 5rem)` | 1.05 | 700 | Home page hero title |
| `text-h1` | `clamp(1.8rem, 4vw, 2.5rem)` | 1.1 | 700 | Page titles |
| `text-h2` | `clamp(1.3rem, 3vw, 1.8rem)` | 1.2 | 600 | Section headings |
| `text-h3` | `1.125rem` | 1.3 | 600 | Card titles, subsections |
| `text-body` | `1rem` | 1.65 | 400 | Body paragraphs |
| `text-body-sm` | `0.875rem` | 1.55 | 400 | Descriptions, secondary text |
| `text-caption` | `0.75rem` | 1.4 | 500 | Labels, metadata, timestamps |
| `text-mono` | `0.875rem` | 1.4 | 400 | Code, scores, data values |

Editorial headlines and UI text use Geist Sans. Data values use Geist Mono.

---

## Spacing System

Base unit: `4px`

| Token | Value | Usage |
|---|---|---|
| `space-1` | `4px` | Tight gaps, icon padding |
| `space-2` | `8px` | Inline spacing, small gaps |
| `space-3` | `12px` | Component internal padding |
| `space-4` | `16px` | Card padding, section gaps |
| `space-5` | `20px` | Medium section gaps |
| `space-6` | `24px` | Large section gaps |
| `space-8` | `32px` | Page section spacing |
| `space-10` | `40px` | Major section breaks |
| `space-12` | `48px` | Hero padding |
| `space-16` | `64px` | Page-level vertical rhythm |

**Principle**: Generous whitespace. Editorial magazine feel. Never crowd elements.

---

## Shape & Elevation

### Border Radius

| Token | Value | Usage |
|---|---|---|
| `radius-sm` | `6px` | Buttons, inputs, small elements |
| `radius-md` | `10px` | Cards, panels |
| `radius-lg` | `16px` | Large cards, modals |
| `radius-xl` | `24px` | Hero banners, feature cards |
| `radius-full` | `9999px` | Pills, badges, avatars |

### Shadows & Glows

| Token | Value | Usage |
|---|---|---|
| `shadow-sm` | `0 1px 2px rgba(0,0,0,0.1)` | Subtle elevation |
| `shadow-md` | `0 4px 12px rgba(0,0,0,0.15)` | Card hover |
| `shadow-lg` | `0 8px 30px rgba(0,0,0,0.2)` | Elevated modals |
| `glow-amber` | `0 0 20px rgba(245, 158, 11, 0.15)` | Amber accent glow |
| `glow-teal` | `0 0 20px rgba(45, 212, 191, 0.12)` | Teal accent glow |

### Glassmorphism Spec

```css
.glass {
  background: var(--surface-primary);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid var(--border-subtle);
}
```

---

## Motion & Animation

### Timing

| Token | Value | Usage |
|---|---|---|
| `duration-fast` | `150ms` | Button hovers, color changes |
| `duration-normal` | `250ms` | Card hovers, tab switches |
| `duration-slow` | `400ms` | Page transitions, modal opens |
| `duration-slower` | `600ms` | Scroll reveals, hero animations |

### Easing

| Token | Value | Usage |
|---|---|---|
| `ease-default` | `cubic-bezier(0.4, 0, 0.2, 1)` | General transitions |
| `ease-in-out` | `cubic-bezier(0.4, 0, 0.2, 1)` | Symmetric animations |
| `ease-out` | `cubic-bezier(0, 0, 0.2, 1)` | Entrance animations |
| `ease-spring` | `cubic-bezier(0.34, 1.56, 0.64, 1)` | Playful bounces (use sparingly) |

### Principles

- **Slow and deliberate** — like celestial movements, not UI jank
- **Subtle, never flashy** — motion supports content, never distracts
- **Respect reduced motion** — wrap animations in `@media (prefers-reduced-motion: no-preference)`
- **Staggered reveals** — lists animate in sequence with 50ms delay between items

---

## Component Primitives Guidelines

### Button

| Variant | Background | Text | Border | Hover |
|---|---|---|---|---|
| Primary | `accent-primary-500` | `text-inverse` | none | `accent-primary-400` + glow |
| Secondary | transparent | `text-primary` | `border-subtle` | `surface-elevated` |
| Ghost | transparent | `text-muted` | none | `text-primary` |
| Outline | transparent | `accent-primary-500` | `accent-primary-500` | `accent-primary-500` bg + inverse text |

- Padding: `10px 16px`
- Border radius: `radius-sm`
- Font: Geist Sans, `text-body-sm`, weight 500
- Icon + text gap: `space-2`

### Badge

| Variant | Background | Text | Border |
|---|---|---|---|
| Default | `surface-secondary` | `text-muted` | `border-subtle` |
| Validation | `rgba(59, 130, 246, 0.12)` | `#60a5fa` | none |
| Official | `rgba(245, 158, 11, 0.12)` | `#fbbf24` | none |
| Success | `rgba(16, 185, 129, 0.12)` | `#34d399` | none |
| Error | `rgba(244, 63, 94, 0.12)` | `#fb7185` | none |
| Warning | `rgba(245, 158, 11, 0.12)` | `#fbbf24` | none |

- Padding: `4px 10px`
- Border radius: `radius-full`
- Font: Geist Sans, `text-caption`, weight 500

### Card

- Background: `surface-primary`
- Border: `1px solid var(--border-subtle)`
- Border radius: `radius-md`
- Padding: `space-4` to `space-6`
- Hover: `translateY(-2px)` + `shadow-md` + border brightens

### Table

- Header: `text-caption`, `text-muted`, uppercase, letter-spacing `0.05em`
- Rows: `text-body-sm`, `text-primary`
- Row hover: `surface-secondary`
- Border: horizontal only, `border-subtle`
- Cell padding: `12px 16px`

### Tabs (Underline Style)

- Inactive: `text-muted`, no underline
- Active: `text-primary`, `2px` amber underline
- Underline transition: `duration-normal ease-default`
- Tab gap: `space-6`

---

## Usage Examples

### Correct

- Amber accent for primary CTAs and active states only
- Glassmorphism cards on dark gradient backgrounds
- Sans-serif typography for challenge titles and editorial content
- Generous whitespace between sections
- Teal for data links and success indicators

### Incorrect

- Amber as a background color (too intense)
- Solid opaque panels without blur (loses depth)
- Mixed serif/sans headline systems that make the product feel inconsistent
- Crowded layouts with tight margins
- Using red/green alone for status (always pair with icon or text)

---

## Dark Mode as Primary

Dark mode is the **primary designed experience**. Light mode is a polished variant, not an afterthought.

**Dark mode first** in all design decisions. When implementing:
1. Design for dark mode
2. Derive light mode by inverting luminance, not by swapping arbitrary colors
3. Verify both modes meet accessibility standards
