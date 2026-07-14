<!--
  The Murmer logo: an "M" cut out of a rounded tile as negative space.

  There are two variants — a lime tile with a dark mark (dark theme) and a
  near-white tile with a green mark (light theme). Rather than branch in
  script, both are expressed through the `--color-brand-tile` /
  `--color-brand-mark` tokens from `+layout.svelte`, which the `data-theme`
  attribute already switches. The variant therefore follows the app theme for
  free, and the same source of truth drives `static/logo/murmer-*.svg` (the
  favicon) and `src-tauri/icons/tray-*.png` (the tray).

  Props:
  - `size`   — rendered edge length in px (the artwork is resolution-free).
  - `wordmark` — also render the "MURMER" lettering next to the mark.
-->
<script lang="ts">
  export let size = 32;
  export let wordmark = false;
</script>

<span class="logo" class:with-wordmark={wordmark}>
  <svg
    width={size}
    height={size}
    viewBox="0 0 128 128"
    role="img"
    aria-label={wordmark ? null : 'Murmer'}
    aria-hidden={wordmark ? 'true' : null}
  >
    <rect width="128" height="128" rx="30" fill="var(--color-brand-tile)" />
    <!-- Single closed path: the M's counters are the tile showing through. -->
    <path
      transform="translate(24 24) scale(0.8)"
      d="M 18 82 L 18 18 L 38 18 L 50 44 L 62 18 L 82 18 L 82 82 L 62 82 L 62 56 L 50 78 L 38 56 L 38 82 Z"
      fill="var(--color-brand-mark)"
    />
  </svg>
  {#if wordmark}
    <span class="wordmark">MURMER</span>
  {/if}
</span>

<style>
  .logo {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
  }

  .with-wordmark {
    gap: var(--space-3);
  }

  .wordmark {
    /* The design sets the wordmark in Manrope ExtraBold; Inter at its heaviest
       weight with the same negative tracking is the closest match without
       shipping a second UI font. */
    font-weight: 800;
    font-size: calc(var(--text-2xl) * 1.1);
    letter-spacing: -0.03em;
    color: var(--color-on-surface);
    line-height: 1;
  }
</style>
