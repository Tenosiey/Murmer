# UI Design System - Murmer Client

## Overview

This document outlines the comprehensive design system implemented for the Murmer desktop chat client. The system follows **Material 3 design principles** with custom enhancements for a modern, professional appearance.

## Design Philosophy

### Core Principles

1. **Consistency** - Unified visual language across all components
2. **Accessibility** - WCAG 2.1 AA compliant, keyboard navigable
3. **Performance** - Optimized animations with respect for reduced motion preferences
4. **Modularity** - Reusable components that compose well together
5. **Modern Aesthetics** - Clean, contemporary design with subtle depth

---

## Color System

### Material 3 Design Tokens

The app uses a comprehensive set of semantic color tokens that adapt to light and dark themes:

#### Dark Theme (Default)
- **Primary**: `#c4b5ff` - Main brand color, used for primary actions
- **Secondary**: `#8bd3ff` - Accent color for secondary elements
- **Tertiary**: `#ffb3c8` - Supporting accent
- **Success**: `#4ade80` - Positive actions/status
- **Warning**: `#facc15` - Caution states
- **Error**: `#ffb4ab` - Error states and destructive actions
- **Surface**: Layered surfaces from `#0f1118` to `#2b2f3d`
- **On-Surface**: `#e8ecff` - Text on surfaces

#### Light Theme
- **Primary**: `#6550db`
- **Secondary**: `#3864ff`
- **Surface**: Layered surfaces from `#f6f6ff` to `#ffffff`
- **On-Surface**: `#0f172a`

### Usage Guidelines

```svelte
<!-- Use color tokens instead of hardcoded colors -->
<div style="color: var(--md-sys-color-primary)">Text</div>
<div style="background: var(--md-sys-color-surface-container-high)">Card</div>

<!-- Use color-mix for transparency -->
<div style="background: color-mix(in srgb, var(--md-sys-color-primary) 20%, transparent)">
  Tinted surface
</div>
```

---

## Typography

### Font Stack

```css
--font-sans: 'Plus Jakarta Sans', 'Inter', 'Segoe UI', sans-serif;
--font-mono: 'JetBrains Mono', 'Fira Code', 'Menlo', monospace;
```

### Type Scale

- **Headline Hero**: `clamp(2.3rem, 5vw, 3.2rem)` - Main page titles
- **Headline**: `1.5rem` - Section headings
- **Body**: `1rem` - Default text
- **Caption**: `0.85rem` - Supporting text
- **Overline**: `0.72rem` - Labels and metadata

### Font Features

```css
font-feature-settings: 'ss01' 1, 'ss03' 1;
-webkit-font-smoothing: antialiased;
```

---

## Spacing & Layout

### Spacing Scale

```css
--radius-xs: 8px;
--radius-sm: 12px;
--radius-md: 16px;
--radius-lg: 22px;
```

### Container Widths

- **Page Container**: `min(1100px, 100%)` - Main content wrapper
- **Card Max Width**: `420px - 460px` - Component cards
- **Content Max Width**: `560px` - Text content

### Responsive Padding

Use `clamp()` for fluid spacing:
```css
padding: clamp(1.5rem, 4vw, 3.5rem);
gap: clamp(2rem, 5vw, 3.5rem);
```

---

## Elevation & Shadows

### Shadow Tokens

```css
--shadow-01: 0 1px 2px rgba(10, 13, 24, 0.25);
--shadow-02: 0 12px 32px rgba(11, 15, 30, 0.28);
--shadow-03: 0 28px 48px rgba(7, 11, 28, 0.32);
```

### Blur Effects

```css
--blur-elevated: saturate(140%) blur(26px);  /* Dark theme */
--blur-elevated: saturate(120%) blur(18px);  /* Light theme */
```

---

## Motion & Animation

### Duration Tokens

```css
--motion-duration-short: 140ms;
--motion-duration-medium: 220ms;
```

### Easing

```css
--motion-easing-standard: cubic-bezier(0.2, 0, 0, 1);
```

### Animation Guidelines

1. **Micro-interactions**: 140ms for hovers, clicks
2. **Transitions**: 220ms for state changes, reveals
3. **Page transitions**: 400-500ms for major UI changes
4. **Always respect** `prefers-reduced-motion`

### Utility Classes

```html
<!-- Fade in animation -->
<div class="fade-in">Content</div>

<!-- Slide up animation -->
<div class="slide-up">Content</div>

<!-- Scale in animation -->
<div class="scale-in">Content</div>

<!-- Loading shimmer -->
<div class="shimmer">Loading skeleton</div>
```

---

## Component Library

### Button Component

Location: `src/lib/components/ui/Button.svelte`

```svelte
<script>
  import { Button } from '$lib/components/ui';
</script>

<!-- Primary button -->
<Button variant="primary" on:click={handleClick}>
  Save Changes
</Button>

<!-- Secondary button -->
<Button variant="secondary" size="sm">
  Cancel
</Button>

<!-- Ghost button -->
<Button variant="ghost" disabled>
  Disabled
</Button>

<!-- Danger button -->
<Button variant="danger" size="lg">
  Delete Account
</Button>

<!-- Icon button -->
<Button icon ariaLabel="Settings">
  ⚙️
</Button>

<!-- Full width -->
<Button fullWidth>
  Continue
</Button>
```

**Props:**
- `variant`: 'primary' | 'secondary' | 'ghost' | 'danger'
- `size`: 'sm' | 'md' | 'lg'
- `type`: 'button' | 'submit' | 'reset'
- `disabled`: boolean
- `fullWidth`: boolean
- `icon`: boolean
- `ariaLabel`: string (required for icon buttons)

### Badge Component

Location: `src/lib/components/ui/Badge.svelte`

```svelte
<script>
  import { Badge } from '$lib/components/ui';
</script>

<!-- Status badges -->
<Badge variant="success">Online</Badge>
<Badge variant="warning">Away</Badge>
<Badge variant="error">Offline</Badge>

<!-- Dot indicators -->
<Badge variant="success" dot pulse />

<!-- Count badge -->
<Badge variant="primary" size="sm">3</Badge>
```

**Props:**
- `variant`: 'primary' | 'secondary' | 'success' | 'warning' | 'error' | 'neutral'
- `size`: 'sm' | 'md' | 'lg'
- `dot`: boolean - Shows as a small dot
- `pulse`: boolean - Adds pulse animation

### Card Component

Location: `src/lib/components/ui/Card.svelte`

```svelte
<script>
  import { Card } from '$lib/components/ui';
</script>

<!-- Default card -->
<Card padding="md">
  <h3>Card Title</h3>
  <p>Card content goes here</p>
</Card>

<!-- Elevated card -->
<Card variant="elevated" padding="lg">
  Premium content
</Card>

<!-- Interactive card -->
<Card interactive on:click={handleClick}>
  Clickable card
</Card>

<!-- Card with glow -->
<Card glowColor="rgba(137, 112, 255, 0.3)">
  Glowing card
</Card>
```

**Props:**
- `variant`: 'default' | 'elevated' | 'outlined' | 'tonal'
- `padding`: 'none' | 'sm' | 'md' | 'lg'
- `interactive`: boolean
- `glowColor`: string | null

### IconButton Component

Location: `src/lib/components/ui/IconButton.svelte`

```svelte
<script>
  import { IconButton } from '$lib/components/ui';
</script>

<IconButton ariaLabel="Settings" on:click={openSettings}>
  <svg><!-- icon --></svg>
</IconButton>

<IconButton variant="primary" size="lg" ariaLabel="Add">
  +
</IconButton>

<IconButton variant="danger" ariaLabel="Delete">
  🗑️
</IconButton>
```

**Props:**
- `variant`: 'default' | 'primary' | 'danger'
- `size`: 'sm' | 'md' | 'lg'
- `ariaLabel`: string (required)
- `type`: 'button' | 'submit' | 'reset'
- `disabled`: boolean

### Tooltip Component

Location: `src/lib/components/ui/Tooltip.svelte`

```svelte
<script>
  import { Tooltip } from '$lib/components/ui';
</script>

<Tooltip text="This is a helpful tooltip" position="top">
  <button>Hover me</button>
</Tooltip>
```

**Props:**
- `text`: string (required)
- `position`: 'top' | 'bottom' | 'left' | 'right'
- `delay`: number (milliseconds, default: 600)

### Spinner Component

Location: `src/lib/components/ui/Spinner.svelte`

```svelte
<script>
  import { Spinner } from '$lib/components/ui';
</script>

<Spinner size="md" color="primary" />
<Spinner size="lg" color="secondary" ariaLabel="Loading data" />
```

**Props:**
- `size`: 'sm' | 'md' | 'lg' | 'xl'
- `color`: 'primary' | 'secondary' | 'on-surface'
- `ariaLabel`: string (default: 'Loading')

---

## Enhanced Components

### ConnectionBars

**Improvements:**
- Material 3 color tokens
- Smooth stagger animations
- Color-coded by signal strength
- Gradient fills for active bars

### PingDot

**Improvements:**
- 6 status levels (unknown, excellent, good, fair, poor, critical)
- Pulse animation for active connections
- Semantic color mapping
- Size variants

### StatusDot

**Improvements:**
- Three states: online, offline, checking
- Pulse animation for online status
- Fade animation for checking state
- ARIA labels for accessibility

### LinkPreview

**Improvements:**
- Enhanced YouTube previews with hover effects
- Material 3 surfaces and borders
- Smooth transitions
- Better contrast and spacing

### ContextMenu

**Improvements:**
- Fly-in animation
- Smart viewport positioning
- Keyboard navigation (Escape to close)
- Hover indicator bar animation
- Support for icons and danger states

---

## Utility Classes

### Surface Classes

```html
<!-- Elevated card surface -->
<div class="surface-card">Content</div>

<!-- Tonal surface (slightly tinted) -->
<div class="surface-tonal">Content</div>

<!-- Outlined surface -->
<div class="surface-outline">Content</div>
```

### Typography Classes

```html
<!-- Eyebrow (small uppercase label) -->
<span class="eyebrow">Category</span>

<!-- Hero headline -->
<h1 class="headline-hero">Welcome to Murmer</h1>

<!-- Muted body text -->
<p class="body-muted">Supporting text</p>
```

### Layout Classes

```html
<!-- Page container with responsive padding -->
<main class="page-container">
  <!-- Content -->
</main>
```

---

## Accessibility Features

### Keyboard Navigation

All interactive components support:
- Tab navigation
- Enter/Space activation
- Escape to close modals/menus
- Arrow keys where appropriate

### Focus Indicators

```css
:focus-visible {
  outline: 3px solid color-mix(in srgb, var(--md-sys-color-secondary) 55%, transparent);
  outline-offset: 2px;
}
```

### Screen Reader Support

- Semantic HTML elements
- ARIA labels on icon-only buttons
- `role` attributes for custom components
- Status announcements with `aria-live`
- `sr-only` class for screen reader-only text

### Color Contrast

All color combinations meet WCAG 2.1 AA standards:
- **Normal text**: 4.5:1 minimum
- **Large text**: 3:1 minimum
- **Interactive elements**: Clear focus indicators

---

## Responsive Design

### Breakpoints

```css
/* Mobile first approach */
@media (max-width: 640px) { /* Mobile */ }
@media (max-width: 720px) { /* Small tablets */ }
@media (min-width: 768px) { /* Tablets */ }
@media (min-width: 1024px) { /* Desktop */ }
```

### Fluid Typography & Spacing

Use `clamp()` for smooth scaling:
```css
font-size: clamp(1rem, 2vw, 1.5rem);
padding: clamp(1rem, 3vw, 2rem);
gap: clamp(1.5rem, 4vw, 3rem);
```

---

## Best Practices

### DO ✅

- Use design tokens instead of hardcoded values
- Compose smaller components into larger ones
- Test with keyboard navigation
- Verify color contrast ratios
- Add smooth transitions to state changes
- Use semantic HTML elements
- Provide meaningful ARIA labels

### DON'T ❌

- Hardcode colors like `#fff` or `rgb(255, 255, 255)`
- Create one-off components for reusable patterns
- Forget `aria-label` on icon-only buttons
- Skip focus indicators
- Use `outline: none` without alternative
- Nest interactive elements (button inside button)
- Ignore reduced motion preferences

---

## File Structure

```
murmer_client/src/lib/components/
├── ui/                          # Reusable UI components
│   ├── index.ts                # Component exports
│   ├── Button.svelte
│   ├── Badge.svelte
│   ├── Card.svelte
│   ├── IconButton.svelte
│   ├── Tooltip.svelte
│   └── Spinner.svelte
│
├── ConnectionBars.svelte        # Enhanced components
├── PingDot.svelte
├── StatusDot.svelte
├── LinkPreview.svelte
├── ContextMenu.svelte
└── SettingsModal.svelte
```

---

## Migration Guide

### Updating Existing Components

#### Before:
```svelte
<button style="background: #6550db; color: white;">
  Save
</button>
```

#### After:
```svelte
<Button variant="primary">
  Save
</Button>
```

#### Before:
```svelte
<div style="border-radius: 12px; padding: 1rem; background: #1b1e28;">
  Content
</div>
```

#### After:
```svelte
<Card padding="md">
  Content
</Card>
```

---

## Resources

### Design References

- [Material Design 3](https://m3.material.io/) - Design system foundation
- [WCAG 2.1 Guidelines](https://www.w3.org/WAI/WCAG21/quickref/) - Accessibility standards
- [Plus Jakarta Sans](https://fonts.google.com/specimen/Plus+Jakarta+Sans) - Primary typeface

### Tools

- [Color Contrast Checker](https://webaim.org/resources/contrastchecker/)
- [Accessible Color Palette Builder](https://toolness.github.io/accessible-color-matrix/)

---

## Version History

**v2024.10.30-alpha**
- Complete Material 3 design system implementation
- Reusable UI component library (Button, Badge, Card, IconButton, Tooltip, Spinner)
- Enhanced existing components with animations and Material 3 tokens
- Comprehensive accessibility improvements
- Smooth micro-interactions and transitions
- Full light/dark theme support

---

## Support & Contribution

For questions or suggestions about the design system:
1. Review existing components in `src/lib/components/ui/`
2. Follow the established patterns and principles
3. Ensure all new components are accessible
4. Test with both light and dark themes
5. Document any new patterns in this guide

---

**Last Updated**: October 30, 2024  
**Maintained By**: Murmer Design Team

