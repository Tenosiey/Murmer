<script lang="ts">
  import { userVolumes, setUserVolume } from '$lib/stores/settings';

  export let open: boolean;
  export let x: number;
  export let y: number;
  export let user: string | null;
  export let onClose: () => void;
</script>

{#if open && user}
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <div class="volume-menu-overlay" on:click={onClose}>
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div class="volume-menu" style="left: {x}px; top: {y}px;" on:click|stopPropagation>
      <div class="volume-menu-header">
        <span class="volume-menu-user">{user}</span>
        <span class="volume-menu-title">Volume Control</span>
      </div>
      <div class="volume-menu-content">
        <div class="volume-control-row">
          <span class="volume-icon">🔊</span>
          <input
            class="volume-menu-slider"
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={$userVolumes[user] ?? 1.0}
            on:input={(e) => {
              if (!user) return;
              setUserVolume(user, parseFloat(e.currentTarget.value));
            }}
          />
          <span class="volume-percentage"
            >{Math.round(($userVolumes[user] ?? 1.0) * 100)}%</span
          >
        </div>
        <div class="volume-presets">
          <button class="preset-btn" on:click={() => user && setUserVolume(user, 0)}>Mute</button>
          <button class="preset-btn" on:click={() => user && setUserVolume(user, 0.5)}>50%</button>
          <button class="preset-btn" on:click={() => user && setUserVolume(user, 1.0)}>100%</button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .volume-menu-overlay {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 40;
  }

  .volume-menu {
    position: absolute;
    width: min(320px, 90vw);
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-md);
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .volume-menu-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.5rem;
  }

  .volume-menu-user {
    font-weight: 600;
  }

  .volume-menu-title {
    font-size: 0.85rem;
    color: var(--color-muted);
  }

  .volume-menu-content {
    display: grid;
    gap: 1.1rem;
  }

  .volume-control-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .volume-icon {
    font-size: 1.2rem;
  }

  .volume-menu-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 0.45rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-surface-raised) 88%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    outline: none;
  }

  .volume-menu-slider:focus {
    border-color: color-mix(in srgb, var(--color-secondary) 32%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-secondary) 18%, transparent);
  }

  .volume-menu-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-surface);
    border: 2px solid color-mix(in srgb, var(--color-secondary) 60%, transparent);
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.15);
    margin-top: -7px;
  }

  .volume-menu-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-surface);
    border: 2px solid color-mix(in srgb, var(--color-secondary) 60%, transparent);
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.15);
  }

  .volume-percentage {
    font-size: 0.82rem;
    color: var(--color-muted);
  }

  .volume-presets {
    display: flex;
    gap: 0.5rem;
  }

  .preset-btn {
    flex: 1;
    padding: 0.55rem 0.75rem;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-primary) 10%, transparent);
    color: var(--color-secondary);
  }

  .preset-btn:hover {
    border-color: color-mix(in srgb, var(--color-primary) 28%, transparent);
  }
</style>
