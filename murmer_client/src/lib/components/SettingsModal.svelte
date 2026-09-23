<!--
  The user's own settings: appearance, audio, microphone, hotkeys, identity,
  stats and the app version. Server-wide settings live in the Server
  Dashboard instead.

  Each tab is its own component in ./settings/. All of them stay mounted
  while the modal is open and only render while `active`, so a hotkey
  capture survives switching tabs and ends when the modal closes. Styles
  several tabs share live here, under `.modal-body :global(...)`, and also
  reach `IdentityBackup`, which uses the same setting-group classes.
-->
<script lang="ts">
  import { serverInfo } from '$lib/stores/serverInfo';
  import AppearanceTab from './settings/AppearanceTab.svelte';
  import AudioTab from './settings/AudioTab.svelte';
  import VoiceTab from './settings/VoiceTab.svelte';
  import HotkeysTab from './settings/HotkeysTab.svelte';
  import IdentityTab from './settings/IdentityTab.svelte';
  import StatsTab from './settings/StatsTab.svelte';
  import AboutTab from './settings/AboutTab.svelte';

  interface Props {
    open: boolean;
    close: () => void;
  }

  let { open, close }: Props = $props();

  // Each settings topic lives on its own tab shown in the left rail. Audio is
  // split by direction: "Audio" covers everything you hear, "Voice" everything
  // your microphone sends.
  const TABS = [
    { id: 'appearance', label: 'Appearance' },
    { id: 'audio', label: 'Audio' },
    { id: 'voice', label: 'Microphone & Voice' },
    { id: 'hotkeys', label: 'Hotkeys' },
    { id: 'identity', label: 'Identity' },
    { id: 'stats', label: 'Stats & Privacy' },
    { id: 'about', label: 'About' },
    { id: 'server', label: 'Server', ownerOnly: true }
  ] as const;
  let activeTab: (typeof TABS)[number]['id'] = $state('appearance');

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      close();
    }
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    // Keyboard activation of the focused backdrop only; keystrokes inside
    // the modal content (inputs, buttons) bubble here and must not close it.
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') {
      close();
    }
  }

  let visibleTabs = $derived(TABS.filter((tab) => !('ownerOnly' in tab && tab.ownerOnly) || $serverInfo));
  // If the active tab disappears (e.g. server info clears), fall back to the first.
  $effect(() => {
    if (!visibleTabs.some((tab) => tab.id === activeTab)) {
      activeTab = visibleTabs[0].id;
    }
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal-overlay" onclick={close} onkeydown={handleOverlayKeydown} role="dialog" aria-modal="true" aria-labelledby="settings-title" tabindex="-1">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="modal-content" onclick={(event) => event.stopPropagation()} onkeydown={handleKeydown} role="document" tabindex="0">
      <div class="modal-header">
        <h2 id="settings-title">Settings</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label="Close settings">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="settings-layout">
        <nav class="settings-tabs" aria-label="Settings sections">
          {#each visibleTabs as tab}
            <button
              class="tab-btn"
              class:selected={activeTab === tab.id}
              aria-pressed={activeTab === tab.id}
              onclick={() => (activeTab = tab.id)}
            >{tab.label}</button>
          {/each}
        </nav>

        <div class="modal-body">
        <AppearanceTab active={activeTab === 'appearance'} />
        <AudioTab active={activeTab === 'audio'} />
        <VoiceTab active={activeTab === 'voice'} />
        <HotkeysTab active={activeTab === 'hotkeys'} />
        <IdentityTab active={activeTab === 'identity'} />
        <StatsTab active={activeTab === 'stats'} />
        <AboutTab active={activeTab === 'about'} />

        {#if activeTab === 'server' && $serverInfo}
          <div class="settings-section">
            <h3 class="section-title">Server</h3>
            <div class="setting-group">
              <div class="setting-label">
                Server version
                <span class="setting-value">{$serverInfo.version}</span>
              </div>
              <div class="setting-description">
                Only visible to users with the Owner or Admin role.
              </div>
            </div>
          </div>
        {/if}
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-primary" onclick={close}>Done</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    /* A plain dim instead of a full-screen backdrop blur — blur here is
       very expensive in WebKitGTK (Linux) while the modal is open. */
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    z-index: var(--z-modal);
    animation: fadeIn 0.15s ease-out;
  }

  .modal-content {
    background: var(--color-surface-elevated);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    border: 1px solid var(--color-surface-outline);
    width: min(864px, 94vw);
    height: min(672px, 88vh);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    animation: slideIn 0.18s var(--motion-easing-standard);
  }

  .settings-layout {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .settings-tabs {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-4) var(--space-3);
    border-right: 1px solid var(--color-surface-outline);
    flex-shrink: 0;
    width: 12rem;
    overflow-y: auto;
  }

  .tab-btn {
    text-align: left;
    justify-content: flex-start;
    background: transparent;
    border: none;
    color: var(--color-muted);
    font-weight: 500;
    font-size: var(--text-md);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
  }

  .tab-btn:hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .tab-btn.selected {
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .modal-header h2 {
    font-size: var(--text-lg);
  }

  .modal-body {
    flex: 1;
    min-width: 0;
    padding: var(--space-5);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .modal-footer {
    padding: var(--space-4) var(--space-5);
    border-top: 1px solid var(--color-surface-outline);
    display: flex;
    justify-content: flex-end;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes slideIn {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }

  /* Shared by the tabs in ./settings/, which render inside .modal-body. */
  .modal-body :global(.settings-section) {
    display: grid;
    gap: var(--space-4);
  }

  .modal-body :global(.section-title) {
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-muted);
  }

  .modal-body :global(.setting-group) {
    display: grid;
    gap: var(--space-2);
  }

  .modal-body :global(.setting-label) {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 500;
    color: var(--color-on-surface);
    font-size: var(--text-md);
  }

  .modal-body :global(.setting-value) {
    font-size: var(--text-sm);
    color: var(--color-muted);
    font-family: var(--font-mono);
  }

  .modal-body :global(.setting-description) {
    font-size: var(--text-sm);
    color: var(--color-muted);
    line-height: 1.5;
  }

  .modal-body :global(.slider-container) {
    position: relative;
    height: var(--control-height);
    display: flex;
    align-items: center;
  }

  .modal-body :global(.slider-container::before) {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    height: 4px;
    top: 50%;
    transform: translateY(-50%);
    background: var(--color-surface-raised);
    border-radius: var(--radius-pill);
    pointer-events: none;
  }

  .modal-body :global(.slider-track-fill) {
    position: absolute;
    left: 0;
    height: 4px;
    top: 50%;
    transform: translateY(-50%);
    background: var(--color-primary);
    border-radius: var(--radius-pill);
    pointer-events: none;
    z-index: 1;
  }

  .modal-body :global(.volume-slider) {
    width: 100%;
    height: 100%;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    border: none;
    position: relative;
    z-index: 2;
    margin: 0;
    padding: 0;
    min-height: 0;
    cursor: pointer;
  }

  .modal-body :global(.volume-slider:focus) {
    box-shadow: none;
  }

  .modal-body :global(.volume-slider::-webkit-slider-thumb) {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-on-surface);
    border: 2px solid var(--color-primary);
    cursor: pointer;
  }

  .modal-body :global(.volume-slider::-moz-range-thumb) {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-on-surface);
    border: 2px solid var(--color-primary);
    cursor: pointer;
  }

  .modal-body :global(.volume-slider::-moz-range-track) {
    background: transparent;
    border: none;
  }

  .modal-body :global(.toggle-row) {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    cursor: pointer;
  }

  .modal-body :global(.toggle-row input[type='checkbox']) {
    margin-top: 0.2rem;
    accent-color: var(--color-primary);
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
    cursor: pointer;
  }

  .modal-body :global(.toggle-text) {
    display: grid;
    gap: 0.125rem;
  }

  .modal-body :global(.toggle-label) {
    font-weight: 500;
    font-size: var(--text-md);
    color: var(--color-on-surface);
  }

  .modal-body :global(.toggle-description) {
    font-size: var(--text-sm);
    color: var(--color-muted);
    line-height: 1.4;
  }

  .modal-body :global(.select-container) {
    position: relative;
  }

  .modal-body :global(.device-select) {
    width: 100%;
    appearance: none;
    padding-right: var(--space-6);
    border-radius: var(--radius-md);
  }

  .modal-body :global(.select-arrow) {
    position: absolute;
    right: var(--space-3);
    top: 50%;
    transform: translateY(-50%);
    pointer-events: none;
    color: var(--color-muted);
    display: inline-flex;
  }

</style>
