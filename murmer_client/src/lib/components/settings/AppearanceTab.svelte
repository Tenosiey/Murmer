<script lang="ts">
  import {
    theme,
    accent,
    DEFAULT_ACCENT,
    accentToHex,
    parseThemeCode,
    themeCode,
    type Accent
  } from '$lib/stores/theme';
  import ThemeWheel from '$lib/components/ThemeWheel.svelte';
  import { t } from '$lib/i18n';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  $effect(() => {
    syncHexField($accent);
  });

  // Preset theme colors shown next to the wheel; each is a wheel position.
  // "Lime" is the brand color and the default — both logo variants (#c8ff3e
  // and #84b800) sit on this hue and differ only in lightness, which the
  // wheel does not carry.
  const ACCENT_PRESETS = [
    { name: t('appearance.accent.lime'), hue: DEFAULT_ACCENT.hue, saturation: DEFAULT_ACCENT.saturation },
    { name: t('appearance.accent.sky'), hue: 215, saturation: 78 },
    { name: t('appearance.accent.indigo'), hue: 250, saturation: 68 },
    { name: t('appearance.accent.violet'), hue: 285, saturation: 68 },
    { name: t('appearance.accent.rose'), hue: 340, saturation: 72 },
    { name: t('appearance.accent.ember'), hue: 22, saturation: 80 },
    { name: t('appearance.accent.moss'), hue: 150, saturation: 55 }
  ];

  // The hex field mirrors the wheel. Edits only take effect on Enter or on
  // blur, so a half-typed code never repaints the whole app.
  let hexInput = $state(accentToHex(DEFAULT_ACCENT));
  let hexInvalid = $state(false);

  function syncHexField(value: Accent | null) {
    hexInput = accentToHex(value ?? DEFAULT_ACCENT);
    hexInvalid = false;
  }

  function commitHex() {
    // Also takes a whole theme code pasted from somebody else.
    const code = parseThemeCode(hexInput);
    if (!code) {
      hexInvalid = true;
      return;
    }
    if (code.mode) theme.set(code.mode);
    const parsed = code.accent;
    const current = $accent ?? DEFAULT_ACCENT;
    if (parsed.hue === current.hue && parsed.saturation === current.saturation) {
      // Unchanged: leaving the field must not pin the default palette to a
      // wheel position, which would enable "Reset to default" out of nowhere.
      hexInvalid = false;
      return;
    }
    accent.set(parsed);
    // Re-render from the stored position: the wheel drops the hex's
    // lightness, so the field must show the color that was actually applied.
    syncHexField(parsed);
  }

  let copied = $state(false);

  async function copyThemeCode() {
    try {
      await navigator.clipboard.writeText(themeCode($theme, $accent));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      console.error('Failed to copy theme code', e);
    }
  }

  /**
   * The overlay closes the modal on Enter/Space, so keys typed here must not
   * bubble that far — committing a color would otherwise dismiss settings.
   * Escape still passes through to close the modal.
   */
  function handleHexKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') return;
    event.stopPropagation();
    if (event.key === 'Enter') {
      event.preventDefault();
      commitHex();
    }
  }
</script>

{#if active}
  {@const share = t('appearance.shareHint').split('{code}')}
  <div class="settings-section">
    <h3 class="section-title">{t('settings.tab.appearance')}</h3>

    <div class="setting-group">
      <span class="setting-label" id="theme-mode-label">{t('appearance.theme')}</span>
      <div class="mode-toggle" role="group" aria-labelledby="theme-mode-label">
        <button
          class="btn mode-btn"
          class:selected={$theme === 'dark'}
          aria-pressed={$theme === 'dark'}
          onclick={() => theme.set('dark')}
        >{t('appearance.dark')}</button>
        <button
          class="btn mode-btn"
          class:selected={$theme === 'light'}
          aria-pressed={$theme === 'light'}
          onclick={() => theme.set('light')}
        >{t('appearance.light')}</button>
      </div>
    </div>

    <div class="setting-group">
      <span class="setting-label">{t('appearance.themeColor')}</span>
      <div class="accent-picker">
        <ThemeWheel
          hue={($accent ?? DEFAULT_ACCENT).hue}
          saturation={($accent ?? DEFAULT_ACCENT).saturation}
          onchange={(hue, saturation) => accent.set({ hue, saturation })}
        />
        <div class="accent-side">
          <div class="swatch-grid">
            {#each ACCENT_PRESETS as preset}
              <button
                class="swatch"
                class:selected={$accent?.hue === preset.hue && $accent?.saturation === preset.saturation}
                style={`background: hsl(${preset.hue} ${preset.saturation}% 50%);`}
                title={preset.name}
                aria-label={t('appearance.useColor', { name: preset.name })}
                onclick={() => accent.set({ hue: preset.hue, saturation: preset.saturation })}
              ></button>
            {/each}
          </div>
          <label class="field hex-field">
            <span>{t('appearance.hexField')}</span>
            <input
              class="hex-input"
              class:invalid={hexInvalid}
              type="text"
              maxlength="20"
              spellcheck="false"
              autocomplete="off"
              placeholder="#27c0e8"
              aria-invalid={hexInvalid}
              bind:value={hexInput}
              oninput={() => (hexInvalid = false)}
              onblur={commitHex}
              onkeydown={handleHexKeydown}
            />
          </label>
          <button class="btn reset-accent" onclick={copyThemeCode}>
            {copied ? t('appearance.copied') : t('appearance.copyCode')}
          </button>
          <button class="btn reset-accent" onclick={() => accent.reset()} disabled={$accent === null}>
            {t('appearance.reset')}
          </button>
        </div>
      </div>
      <div class="setting-description">
        {t('appearance.wheelHint')}
        {share[0]}<code>dark #8fbf26</code>{share[1]}
      </div>
    </div>
  </div>
{/if}

<style>

  .mode-toggle {
    display: flex;
    gap: var(--space-2);
  }

  .mode-btn.selected {
    background: var(--color-primary-container);
    border-color: var(--color-primary);
    color: var(--color-primary);
  }

  .accent-picker {
    display: flex;
    align-items: center;
    gap: var(--space-5);
  }

  .accent-side {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .swatch-grid {
    display: grid;
    grid-template-columns: repeat(3, auto);
    gap: var(--space-2);
    justify-content: start;
  }

  .swatch {
    width: 2rem;
    height: 2rem;
    padding: 0;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-surface-outline);
  }

  .swatch.selected {
    box-shadow:
      0 0 0 2px var(--color-surface-elevated),
      0 0 0 4px var(--color-primary);
  }

  .hex-field {
    gap: var(--space-1);
  }

  .hex-input {
    width: 7rem;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .hex-input.invalid {
    border-color: var(--color-warning);
    box-shadow: 0 0 0 1px var(--color-warning);
  }

  .reset-accent {
    align-self: flex-start;
  }
</style>
