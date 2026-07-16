<script lang="ts">

  import { userVolumes, setUserVolume } from '$lib/stores/settings';

  interface Props {
    open: boolean;
    x: number;
    y: number;
    user: string | null;
    onClose: () => void;
  }

  let {
    open,
    x,
    y,
    user,
    onClose
  }: Props = $props();
</script>

{#if open && user}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="volume-menu-overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="volume-menu" style="left: {x}px; top: {y}px;" onclick={(event) => event.stopPropagation()}>
      <div class="volume-menu-header">
        <span class="volume-menu-user">{user}</span>
        <span class="volume-menu-title">Volume Control</span>
      </div>
      <div class="volume-menu-content">
        <div class="volume-control-row">
          <span class="volume-icon" aria-hidden="true">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/></svg>
          </span>
          <input
            class="volume-menu-slider"
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={$userVolumes[user] ?? 1.0}
            oninput={(e) => {
              if (!user) return;
              setUserVolume(user, parseFloat(e.currentTarget.value));
            }}
          />
          <span class="volume-percentage"
            >{Math.round(($userVolumes[user] ?? 1.0) * 100)}%</span
          >
        </div>
        <div class="volume-presets">
          <button class="preset-btn" onclick={() => user && setUserVolume(user, 0)}>Mute</button>
          <button class="preset-btn" onclick={() => user && setUserVolume(user, 0.5)}>50%</button>
          <button class="preset-btn" onclick={() => user && setUserVolume(user, 1.0)}>100%</button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .volume-menu-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-dropdown);
  }

  .volume-menu {
    position: absolute;
    width: min(300px, 90vw);
    background: var(--color-surface-elevated);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-md);
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .volume-menu-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-2);
  }

  .volume-menu-user {
    font-weight: 600;
    font-size: var(--text-md);
  }

  .volume-menu-title {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .volume-menu-content {
    display: grid;
    gap: var(--space-3);
  }

  .volume-control-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .volume-icon {
    display: inline-flex;
    color: var(--color-muted);
    flex-shrink: 0;
  }

  .volume-menu-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 4px;
    min-height: 0;
    padding: 0;
    border-radius: var(--radius-pill);
    background: var(--color-surface-raised);
    border: none;
    outline: none;
  }

  .volume-menu-slider:focus {
    box-shadow: none;
  }

  .volume-menu-slider:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }

  .volume-menu-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-on-surface);
    border: 2px solid var(--color-primary);
    margin-top: -6px;
  }

  .volume-menu-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-on-surface);
    border: 2px solid var(--color-primary);
  }

  .volume-percentage {
    font-size: var(--text-sm);
    font-family: var(--font-mono);
    color: var(--color-muted);
    min-width: 2.75rem;
    text-align: right;
  }

  .volume-presets {
    display: flex;
    gap: var(--space-2);
  }

  .preset-btn {
    flex: 1;
    min-height: 2rem;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-raised);
    color: var(--color-on-surface-variant);
    font-size: var(--text-sm);
  }

  .preset-btn:hover {
    border-color: var(--color-outline-strong);
    color: var(--color-on-surface);
  }
</style>
