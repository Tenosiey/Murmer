# UI conventions

The design system the client is built from. To *write* a component, follow
[`../agents/skills/svelte-ui.md`](../agents/skills/svelte-ui.md); this is the
reference for what exists.

## Design tokens

All tokens are defined in `murmer_client/src/routes/+layout.svelte`:

| Token family | Use |
| --- | --- |
| `--color-*` | Every color. Re-tinted by the user's accent choice. |
| `--space-*` | A 4px scale, used for **all** padding, margin and gap. |
| `--text-*` | Font sizes. |
| `--radius-*` | Corner radii. |
| `--shadow-*` | Elevation. |
| `--control-height*` | Input and button heights. |
| `--z-*` | Stacking order. |

**No hardcoded colors, font sizes or one-off spacing values.** A literal
`12px` or `#2a2a2a` in a component is a bug: it will not follow the theme,
and it will not follow the accent re-tint.

## Shared primitives

Reuse the primitives from the layout instead of restyling buttons and inputs
per component:

`.btn`, `.btn-primary`, `.btn-ghost`, `.btn-danger`, `.icon-btn`, `.field`,
`.menu-panel`, `.badge`, `.surface-card`.

## Type and icons

- UI text uses **Inter** (`--font-sans`).
- **JetBrains Mono** (`--font-mono`) is reserved for code, timestamps and
  server addresses.
- Icons are **inline stroke SVGs at 1.8 stroke width**. No emoji as icons.

## Brand

The logo is an "M" cut as negative space out of a rounded tile, in two
theme-following variants:

| | Tile | Mark |
| --- | --- | --- |
| Dark | `#c8ff3e` | `#141a05` |
| Light | `#f7faee` | `#84b800` |

Both sit on the same hue, which is also the app's default theme color.
Picking any other color on the theme wheel re-tints the UI but **never** the
logo: `--color-brand-tile` and `--color-brand-mark` are deliberately exempt
from the accent re-tinting the rest of `--color-*` gets.

In the app, use `src/lib/components/MurmerLogo.svelte` — it reads those two
tokens and switches with the theme on its own. Never inline the artwork.

The same artwork is duplicated in three places because two of them are
consumed outside the DOM. Keep all three in sync:

| Path | Consumed by |
| --- | --- |
| `src/lib/components/MurmerLogo.svelte` | the app |
| `static/logo/murmer-{dark,light}.svg` | favicon, README |
| `src-tauri/icons/` | installer, window, tray |

Regenerating the icons after changing the artwork is documented in the Brand
section of [`../README.md`](../README.md).
