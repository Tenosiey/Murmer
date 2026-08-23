# Svelte components

Read this before building or editing a component under
`murmer_client/src/lib/components/` or `src/routes/`.

The token and primitive reference is [`../../docs/ui.md`](../../docs/ui.md).
This is the procedure.

## Non-negotiables

- **Runes only.** `$state`, `$props()`, `$derived`, `$effect`. `runes: true`
  in `svelte.config.js` fails the build on `export let` or `$:`, and
  `svelte/legacy` is never imported.
- **Tokens only.** No hardcoded colors, font sizes or one-off spacing. Use
  `--color-*`, `--space-*` (4px scale, for *all* padding/margin/gap),
  `--text-*`, `--radius-*`, `--shadow-*`, `--control-height*`, `--z-*`.
  A literal `#2a2a2a` will not follow the theme; a literal `12px` will not
  follow the spacing scale.
- **Primitives before new styles.** `.btn`, `.btn-primary`, `.btn-ghost`,
  `.btn-danger`, `.icon-btn`, `.field`, `.menu-panel`, `.badge`,
  `.surface-card` are defined in `src/routes/+layout.svelte`. Restyling a
  button locally is how a design system dies.
- **Icons are inline stroke SVGs at 1.8 stroke width.** No emoji as icons.
- **`MurmerLogo.svelte` for the logo**, never inlined artwork.
- **Never `{@html …}`** on anything not explicitly sanitised.
- **Never `window.prompt`, `confirm` or `alert`.** WebView2 does not support
  them at all, so in the desktop app they silently do nothing. Every prompt,
  confirmation, selection and alert goes through `dialogs.*` from
  `src/lib/stores/dialogs.ts`.

## Where the logic goes

Put anything worth testing in a **pure function under `src/lib/`**, and let
the component call it. That is the difference between logic a test can reach
and logic that can only be verified by clicking. `wiki/diff.ts` and
`webrtc/recovery.ts` are the pattern.

Cross-component state goes in a store under `src/lib/stores/`, consumed with
`$store`. Component-local state uses runes. See
[`../../docs/client-state.md`](../../docs/client-state.md).

## Names

Render `$displayNames(user)` from `stores/profiles.ts` — never the raw user
name, and never `profile.displayName` directly. The raw name is the account
name and stays the key for every lookup (`$roles[user]`, `$avatars[user]`, DM
peers, mentions); reaching for `displayName` yourself skips the nickname a
moderator may have set.

## Prefer small components

Murmer is maintained by one or two people. Prefer small, composable
components and a flat folder over a deep hierarchy. Sections of the chat page
live in `src/lib/components/chat/`.

Some things are deliberately *not* extracted — the scroll logic in
`routes/chat/+page.svelte` is the standing example. If a refactor looks
tempting there, ask first.

## Before finishing

1. `bun run check` — 0 errors, **0 warnings**.
2. `bun run test`.
3. **Look at it running.** Component rendering tests do not exist here on
   purpose; the check for clipping, overlap, spacing, stale state and focus
   is your eyes on the real app. Follow
   [`visual-verification.md`](visual-verification.md).
4. If the component is gated on a permission, confirm the *server* enforces
   the same thing. Client gating is cosmetic — a hidden button is not a
   permission check.
