<script lang="ts">

  import { userVolumes, setUserVolume, MAX_USER_VOLUME } from '$lib/stores/settings';
  import { soundboardPrefs } from '$lib/stores/soundboardSettings';

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

  /** Whether this user's soundboard clips are silenced for us locally. This is
      independent of their voice volume above: someone can be perfectly
      audible while their airhorn is not. */
  let soundsMuted = $derived(user !== null && $soundboardPrefs.mutedUsers.includes(user));

  let userVolume = $derived(user === null ? 1.0 : ($userVolumes[user] ?? 1.0));
  /** Above 100% the stream is amplified, which lifts their background noise
      and any clipping just as much — worth saying out loud. */
  let boosted = $derived(userVolume > 1);
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
            class:boosted
            type="range"
            min="0"
            max={MAX_USER_VOLUME}
            step="0.01"
            aria-label="Volume for {user}"
            value={userVolume}
            oninput={(e) => {
              if (!user) return;
              setUserVolume(user, parseFloat(e.currentTarget.value));
            }}
          />
          <span class="volume-percentage" class:boosted>{Math.round(userVolume * 100)}%</span>
        </div>
        <div class="volume-presets">
          <button class="preset-btn" onclick={() => user && setUserVolume(user, 0)}>Mute</button>
          <button class="preset-btn" onclick={() => user && setUserVolume(user, 0.5)}>50%</button>
          <button class="preset-btn" onclick={() => user && setUserVolume(user, 1.0)}>100%</button>
          <button class="preset-btn" onclick={() => user && setUserVolume(user, MAX_USER_VOLUME)}>
            {Math.round(MAX_USER_VOLUME * 100)}%
          </button>
        </div>
        {#if boosted}
          <p class="volume-hint">Boosted above 100% — amplifies their background noise too.</p>
        {/if}
        <button
          class="preset-btn sound-mute-btn"
          class:muted={soundsMuted}
          onclick={() => user && soundboardPrefs.setUserMuted(user, !soundsMuted)}
        >
          {soundsMuted ? 'Unmute their sounds' : 'Mute their sounds'}
        </button>
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
    /* Tick at the midpoint of the track, which is exactly 100% — it marks
       where the boost range begins. */
    background:
      linear-gradient(var(--color-outline-strong), var(--color-outline-strong)) 50% / 2px 100%
        no-repeat,
      var(--color-surface-raised);
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

  .volume-menu-slider.boosted::-webkit-slider-thumb {
    border-color: var(--color-warning);
  }

  .volume-menu-slider.boosted::-moz-range-thumb {
    border-color: var(--color-warning);
  }

  .volume-percentage {
    font-size: var(--text-sm);
    font-family: var(--font-mono);
    color: var(--color-muted);
    min-width: 2.75rem;
    text-align: right;
  }

  .volume-percentage.boosted {
    color: var(--color-warning);
  }

  .volume-hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--color-muted);
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

  .sound-mute-btn {
    width: 100%;
  }

  .sound-mute-btn.muted {
    border-color: var(--color-primary);
    color: var(--color-primary);
  }
</style>
