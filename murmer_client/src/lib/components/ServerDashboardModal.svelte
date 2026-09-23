<!--
  Server management dashboard for moderators and above. Separate from the
  user-facing SettingsModal: everything in here is server-wide state, gated
  per tab by the permission it controls.

  Every control here is cosmetic twice over: the server re-checks the
  permission behind each frame, and it enforces the settings themselves
  (slow mode, the message cap, the profanity filter, the auto-moderation
  rules, the upload policy). What
  a tab renders is the server's own answer — the identity, chat, upload,
  voice and screen-share frames it broadcasts — never local state that could
  drift from it.

  Each tab is its own component in ./dashboard/. All of them stay mounted
  while the dashboard is open and only render while `active`, so a
  half-edited form survives switching tabs and a server error still reaches
  the tab that caused it. Styles several tabs share live here, under
  `.modal-body :global(...)`.
-->
<script lang="ts">
  import { PERMISSIONS, hasPermission } from '$lib/chat/permissions';
  import OverviewTab from './dashboard/OverviewTab.svelte';
  import HealthTab from './dashboard/HealthTab.svelte';
  import EmojisTab from './dashboard/EmojisTab.svelte';
  import ModerationTab from './dashboard/ModerationTab.svelte';
  import InvitesTab from './dashboard/InvitesTab.svelte';
  import AuditTab from './dashboard/AuditTab.svelte';
  import StatsTab from './dashboard/StatsTab.svelte';
  import UploadsTab from './dashboard/UploadsTab.svelte';
  import VoiceTab from './dashboard/VoiceTab.svelte';
  import ScreenShareTab from './dashboard/ScreenShareTab.svelte';
  import RolesTab from './dashboard/RolesTab.svelte';
  import DangerTab from './dashboard/DangerTab.svelte';

  interface Props {
    open: boolean;
    close: () => void;
    /** The viewer's effective permission mask; gates which tabs are visible. */
    permissions: number;
  }

  let { open, close, permissions }: Props = $props();

  // Each server-wide topic lives on its own tab, gated by the permission it
  // controls so a role only sees what it can act on.
  const TABS = [
    { id: 'overview', label: 'Overview', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'health', label: 'Health', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'emojis', label: 'Emojis', perm: PERMISSIONS.MANAGE_EMOJIS },
    { id: 'moderation', label: 'Moderation', perm: PERMISSIONS.BAN_MEMBERS },
    { id: 'invites', label: 'Invites', perm: PERMISSIONS.CREATE_INVITES },
    { id: 'audit', label: 'Audit Log', perm: PERMISSIONS.VIEW_AUDIT_LOG },
    { id: 'stats', label: 'Stats', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'uploads', label: 'Files & Uploads', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'voice', label: 'Voice', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'screenshare', label: 'Screen Share', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'roles', label: 'Roles', perm: PERMISSIONS.MANAGE_ROLES },
    { id: 'danger', label: 'Danger Zone', perm: PERMISSIONS.ADMINISTRATOR }
  ] as const;
  let activeTab: (typeof TABS)[number]['id'] = $state('emojis');

  let visibleTabs = $derived(TABS.filter((tab) => hasPermission(permissions, tab.perm)));
  // If the active tab disappears (e.g. the viewer's role changed), fall back
  // to the first visible one.
  $effect(() => {
    if (visibleTabs.length > 0 && !visibleTabs.some((tab) => tab.id === activeTab)) {
      activeTab = visibleTabs[0].id;
    }
  });

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
</script>

{#if open}
  <div class="modal-overlay" onclick={close} onkeydown={handleOverlayKeydown} role="dialog" aria-modal="true" aria-labelledby="server-dashboard-title" tabindex="-1">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="modal-content" onclick={(event) => event.stopPropagation()} onkeydown={handleKeydown} role="document" tabindex="0">
      <div class="modal-header">
        <h2 id="server-dashboard-title">Server Dashboard</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label="Close server dashboard">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="settings-layout">
        <nav class="settings-tabs" aria-label="Server dashboard sections">
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
          <OverviewTab active={activeTab === 'overview'} />
          <HealthTab active={activeTab === 'health'} />
          <EmojisTab active={activeTab === 'emojis'} />
          <ModerationTab active={activeTab === 'moderation'} {permissions} />
          <InvitesTab active={activeTab === 'invites'} />
          <AuditTab active={activeTab === 'audit'} />
          <StatsTab active={activeTab === 'stats'} />
          <UploadsTab active={activeTab === 'uploads'} />
          <VoiceTab active={activeTab === 'voice'} />
          <ScreenShareTab active={activeTab === 'screenshare'} />
          <RolesTab active={activeTab === 'roles'} />
          <DangerTab active={activeTab === 'danger'} />
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
    /* Plain dim instead of a backdrop blur — blur is very expensive in
       WebKitGTK (Linux) while the modal is open. */
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
    display: flex;
    justify-content: flex-end;
    padding: var(--space-3) var(--space-5);
    border-top: 1px solid var(--color-surface-outline);
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
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  /* Shared by the tabs in ./dashboard/, which render inside .modal-body. */
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
    align-items: center;
    gap: var(--space-2);
    font-weight: 500;
    color: var(--color-on-surface);
    font-size: var(--text-md);
  }

  .modal-body :global(.setting-description) {
    font-size: var(--text-sm);
    color: var(--color-muted);
    line-height: 1.5;
  }

  .modal-body :global(.toggle-row) {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .modal-body :global(.toggle-row input[type='checkbox']) {
    margin-top: 0.2rem;
    accent-color: var(--color-primary);
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
  }

  .modal-body :global(.toggle-text) {
    display: grid;
    gap: 0.125rem;
  }

  .modal-body :global(.toggle-label) {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 500;
    font-size: var(--text-md);
    color: var(--color-on-surface);
  }

  .modal-body :global(.toggle-description) {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .modal-body :global(.identity-feedback) {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .modal-body :global(.identity-feedback.error) {
    color: var(--color-warning);
  }

  .modal-body :global(.rule-actions) {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .modal-body :global(.audit-list),
  .modal-body :global(.ban-list),
  .modal-body :global(.online-list),
  .modal-body :global(.storage-list),
  .modal-body :global(.invite-list) {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-1);
  }

  .modal-body :global(.ban-row),
  .modal-body :global(.online-row),
  .modal-body :global(.storage-row) {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }

  .modal-body :global(.ban-row:hover),
  .modal-body :global(.online-row:hover) {
    background: var(--color-surface-raised);
  }

  .modal-body :global(.ban-name),
  .modal-body :global(.online-name) {
    color: var(--color-on-surface);
    font-size: var(--text-sm);
  }

  .modal-body :global(.ban-meta),
  .modal-body :global(.online-account) {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .modal-body :global(.storage-label) {
    font-size: var(--text-sm);
    color: var(--color-on-surface);
    width: 7rem;
    flex-shrink: 0;
  }

  .modal-body :global(.storage-value) {
    font-size: var(--text-sm);
    color: var(--color-muted);
    width: 6.5rem;
    text-align: right;
    flex-shrink: 0;
  }

</style>
