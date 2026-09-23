<script lang="ts">
  import { theme, accent, DEFAULT_ACCENT, accentToHex, hexToAccent, type Accent } from '$lib/stores/theme';
  import ThemeWheel from '$lib/components/ThemeWheel.svelte';

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
    { name: 'Lime', hue: DEFAULT_ACCENT.hue, saturation: DEFAULT_ACCENT.saturation },
    { name: 'Sky', hue: 215, saturation: 78 },
    { name: 'Indigo', hue: 250, saturation: 68 },
    { name: 'Violet', hue: 285, saturation: 68 },
    { name: 'Rose', hue: 340, saturation: 72 },
    { name: 'Ember', hue: 22, saturation: 80 },
    { name: 'Moss', hue: 150, saturation: 55 }
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
    const parsed = hexToAccent(hexInput);
    if (!parsed) {
      hexInvalid = true;
      return;
    }
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
  <div class="settings-section">
    <h3 class="section-title">Appearance</h3>

    <div class="setting-group">
      <span class="setting-label" id="theme-mode-label">Theme</span>
      <div class="mode-toggle" role="group" aria-labelledby="theme-mode-label">
        <button
          class="btn mode-btn"
          class:selected={$theme === 'dark'}
          aria-pressed={$theme === 'dark'}
          onclick={() => theme.set('dark')}
        >Dark</button>
        <button
          class="btn mode-btn"
          class:selected={$theme === 'light'}
          aria-pressed={$theme === 'light'}
          onclick={() => theme.set('light')}
        >Light</button>
      </div>
    </div>

    <div class="setting-group">
      <span class="setting-label">Theme color</span>
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
                aria-label={`Use ${preset.name} theme color`}
                onclick={() => accent.set({ hue: preset.hue, saturation: preset.saturation })}
              ></button>
            {/each}
          </div>
          <label class="field hex-field">
            <span>Hex code</span>
            <input
              class="hex-input"
              class:invalid={hexInvalid}
              type="text"
              maxlength="7"
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
          <button class="btn reset-accent" onclick={() => accent.reset()} disabled={$accent === null}>
            Reset to default
          </button>
        </div>
      </div>
      <div class="setting-description">
        Drag the dot to recolor the whole app — the angle picks the color, the distance from the center picks how strong it is.
        You can also type a hex code; its brightness is set by the theme, so only the color and its strength are taken from it.
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
