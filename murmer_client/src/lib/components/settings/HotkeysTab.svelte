<script lang="ts">
  import { onDestroy } from 'svelte';
  import {
    hotkeys,
    HOTKEY_ACTIONS,
    globalHotkeysEnabled,
    eventToCombo,
    formatCombo,
    type HotkeyActionId
  } from '$lib/stores/hotkeys';
  import { suspendGlobalHotkeys, resumeGlobalHotkeys } from '$lib/stores/globalHotkeys';
  import { isTauri } from '$lib/platform';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let capturingHotkeyId: HotkeyActionId | null = $state(null);
  let hotkeyCaptureCleanup: (() => void) | null = null;

  function stopHotkeyCapture() {
    hotkeyCaptureCleanup?.();
    hotkeyCaptureCleanup = null;
    if (capturingHotkeyId !== null) {
      capturingHotkeyId = null;
      resumeGlobalHotkeys();
    }
  }

  /**
   * Grab the next full key combination and bind it to the action. Runs in
   * the capture phase so the press can't leak into the app (e.g. Escape
   * closing the modal); modifier-only presses keep the capture open,
   * Escape cancels it.
   */
  function captureHotkey(id: HotkeyActionId) {
    if (capturingHotkeyId === id) {
      stopHotkeyCapture();
      return;
    }
    stopHotkeyCapture();
    capturingHotkeyId = id;
    // Release OS-level shortcuts while capturing: a registered combo would
    // be consumed by the OS and could never be captured (or re-assigned).
    suspendGlobalHotkeys();
    const handler = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === 'Escape') {
        stopHotkeyCapture();
        return;
      }
      const combo = eventToCombo(event);
      if (!combo) return;
      hotkeys.bind(id, combo);
      stopHotkeyCapture();
    };
    document.addEventListener('keydown', handler, true);
    hotkeyCaptureCleanup = () => document.removeEventListener('keydown', handler, true);
  }

  onDestroy(stopHotkeyCapture);
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Hotkeys</h3>

    <div class="setting-group">
      {#each HOTKEY_ACTIONS as action (action.id)}
        <div class="hotkey-row">
          <span class="toggle-text">
            <span class="toggle-label">
              {action.label}
              {#if action.global}
                <span class="global-badge" title="Also works while another app is focused">system-wide</span>
              {/if}
            </span>
            <span class="toggle-description">{action.description}</span>
          </span>
          <div class="hotkey-controls">
            <button
              class="btn hotkey-btn"
              class:capturing={capturingHotkeyId === action.id}
              class:unset={!$hotkeys[action.id] && capturingHotkeyId !== action.id}
              onclick={() => captureHotkey(action.id)}
              title="Click, then press the new key combination"
            >
              <span class="hotkey-combo">
                {#if capturingHotkeyId === action.id}
                  Press keys…
                {:else}
                  {formatCombo($hotkeys[action.id])}
                {/if}
              </span>
            </button>
            <button
              class="icon-btn hotkey-clear"
              onclick={() => hotkeys.unbind(action.id)}
              disabled={!$hotkeys[action.id]}
              title="Remove hotkey"
              aria-label={`Remove hotkey for ${action.label}`}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18"></line>
                <line x1="6" y1="6" x2="18" y2="18"></line>
              </svg>
            </button>
          </div>
        </div>
      {/each}

      <div class="setting-description">
        Click a hotkey and press the new key combination — Esc cancels, and assigning a
        combination that is already in use moves it to the new action. Hotkeys without
        Ctrl, Alt or a function key stay inactive while you are typing a message. The
        push-to-talk key is configured in the Microphone &amp; Voice tab.
      </div>

      {#if isTauri}
      <label class="toggle-row">
        <input type="checkbox" bind:checked={$globalHotkeysEnabled} />
        <span class="toggle-text">
          <span class="toggle-label">System-wide voice hotkeys</span>
          <span class="toggle-description">
            Keep the hotkeys marked "system-wide" working while another application is
            focused, e.g. to mute your microphone during a game. Note that Murmer then
            reserves those key combinations for itself while it is running.
          </span>
        </span>
      </label>
      {/if}

      <button class="btn reset-hotkeys" onclick={() => hotkeys.resetAll()}>
        Reset to defaults
      </button>
    </div>
  </div>
{/if}

<style>

  .hotkey-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .hotkey-controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  /* A fixed width, not a minimum: the combos differ in length ("F1" against
     "Ctrl + Shift + M") and a shrink-to-fit button leaves every row a
     different size. Anything longer ellipsises instead of widening. */
  .hotkey-btn {
    width: 10.5rem;
    flex-shrink: 0;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .hotkey-combo {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hotkey-btn.capturing {
    border-color: var(--color-warning);
    color: var(--color-warning);
  }

  .hotkey-btn.unset {
    color: var(--color-muted);
    font-style: italic;
    font-family: inherit;
  }

  .hotkey-clear {
    border: 1px solid var(--color-surface-outline);
  }

  .hotkey-clear:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .global-badge {
    margin-left: var(--space-2);
    padding: 0.0625rem var(--space-2);
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-raised);
    color: var(--color-muted);
    font-size: var(--text-xs);
    font-weight: 500;
    vertical-align: middle;
  }

  .reset-hotkeys {
    justify-self: start;
  }
</style>
