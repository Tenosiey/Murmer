<!--
  Chat page header: channel name/topic, own status menu, connection info,
  theme/notification toggles and the search/settings/leave/logout actions.
-->
<script lang="ts">
  import PingDot from '$lib/components/PingDot.svelte';
  import ConnectionBars from '$lib/components/ConnectionBars.svelte';
  import ConnectionStatsPanel from '$lib/components/ConnectionStatsPanel.svelte';
  import { ping } from '$lib/stores/ping';
  import { session } from '$lib/stores/session';
  import { theme } from '$lib/stores/theme';
  import { statuses, STATUS_LABELS, USER_STATUS_VALUES } from '$lib/stores/status';
  import { channelNotifications, type ChannelNotificationPreference } from '$lib/stores/channelNotifications';
  import { NOTIFICATION_OPTIONS } from '$lib/chat/constants';
  import { ensureStatus } from '$lib/chat/helpers';
  import type { UserStatus } from '$lib/types';

  export let channelId: number;
  export let channelName: string;
  export let topic: string;
  export let serverStrength: number;
  export let statusMap: Record<string, UserStatus>;
  export let onEditTopic: () => void;
  export let onOpenSearch: () => void;
  export let onOpenSettings: () => void;
  export let showServerDashboard = false;
  export let onOpenServerDashboard: () => void = () => {};
  export let onLeaveServer: () => void;
  export let onLogout: () => void;

  const statusOptions: Array<{ value: UserStatus; label: string }> =
    USER_STATUS_VALUES.map((value) => ({
      value,
      label: STATUS_LABELS[value]
    }));

  let statusMenuOpen = false;
  let statusMenuButton: HTMLButtonElement | null = null;
  let statusMenuElement: HTMLDivElement | null = null;

  let notificationMenuOpen = false;
  let notificationMenuButton: HTMLButtonElement | null = null;
  let notificationMenuElement: HTMLDivElement | null = null;

  let statsMenuOpen = false;
  let statsMenuButton: HTMLButtonElement | null = null;
  let statsMenuElement: HTMLDivElement | null = null;

  $: currentUserStatus = $session.user
    ? ensureStatus(statusMap, $session.user, 'online')
    : 'offline';
  $: currentUserStatusLabel = STATUS_LABELS[currentUserStatus];

  $: currentNotificationPreference = ($channelNotifications[channelId] ?? 'all') as ChannelNotificationPreference;
  $: notificationMenuLabel = (() => {
    const found = NOTIFICATION_OPTIONS.find((option) => option.value === currentNotificationPreference);
    return found ? found.label : 'All messages';
  })();

  // Close any open menu when the user switches channels.
  let lastChannelId = channelId;
  $: if (channelId !== lastChannelId) {
    lastChannelId = channelId;
    statusMenuOpen = false;
    notificationMenuOpen = false;
    statsMenuOpen = false;
  }

  function toggleStatusMenu(event: MouseEvent) {
    event.stopPropagation();
    if (notificationMenuOpen) {
      notificationMenuOpen = false;
    }
    statsMenuOpen = false;
    statusMenuOpen = !statusMenuOpen;
  }

  function toggleStatsMenu(event: MouseEvent) {
    event.stopPropagation();
    statusMenuOpen = false;
    notificationMenuOpen = false;
    statsMenuOpen = !statsMenuOpen;
  }

  function selectStatus(value: UserStatus) {
    statuses.setSelf(value);
    statusMenuOpen = false;
  }

  function toggleNotificationMenu(event: MouseEvent) {
    event.stopPropagation();
    if (statusMenuOpen) {
      statusMenuOpen = false;
    }
    notificationMenuOpen = !notificationMenuOpen;
  }

  function selectNotificationPreference(value: ChannelNotificationPreference) {
    channelNotifications.setPreference(channelId, value);
    notificationMenuOpen = false;
  }

  function handleMenuOutside(event: MouseEvent) {
    const target = event.target as Node | null;
    if (statusMenuOpen) {
      if (statusMenuElement && target && statusMenuElement.contains(target)) return;
      if (statusMenuButton && target && statusMenuButton.contains(target)) return;
      statusMenuOpen = false;
    }
    if (notificationMenuOpen) {
      if (notificationMenuElement && target && notificationMenuElement.contains(target)) return;
      if (notificationMenuButton && target && notificationMenuButton.contains(target)) return;
      notificationMenuOpen = false;
    }
    if (statsMenuOpen) {
      if (statsMenuElement && target && statsMenuElement.contains(target)) return;
      if (statsMenuButton && target && statsMenuButton.contains(target)) return;
      statsMenuOpen = false;
    }
  }

  function handleStatsMenuKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      statsMenuOpen = false;
      event.stopPropagation();
      event.preventDefault();
    }
  }

  function handleStatusMenuKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      statusMenuOpen = false;
      event.stopPropagation();
      event.preventDefault();
    }
  }

  function handleNotificationMenuKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      notificationMenuOpen = false;
      event.stopPropagation();
      event.preventDefault();
    }
  }
</script>

<svelte:window on:click={handleMenuOutside} />

<div class="header">
  <div class="title">
    <h1>{channelName}</h1>
    {#if topic}
      <p class="topic" title={topic}>{topic}</p>
    {:else}
      <p class="topic empty">No topic set</p>
    {/if}
  </div>
  <div class="actions">
    <div class="action-group">
      <div class="user">{$session.user}</div>
      <div class="status-control">
      <button
        class="btn btn-ghost status-button"
        bind:this={statusMenuButton}
        aria-haspopup="true"
        aria-expanded={statusMenuOpen}
        on:click={toggleStatusMenu}
        title={`Set status (${currentUserStatusLabel})`}
      >
        <span class={`status ${currentUserStatus}`}></span>
        <span class="status-button-label">{currentUserStatusLabel}</span>
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <polyline points="6 9 12 15 18 9"></polyline>
        </svg>
      </button>
      {#if statusMenuOpen}
        <div
          class="status-menu"
          bind:this={statusMenuElement}
          role="menu"
          tabindex="-1"
          on:click|stopPropagation
          on:keydown={handleStatusMenuKeydown}
        >
          {#each statusOptions as option}
            <button
              class:active={option.value === currentUserStatus}
              on:click={() => selectStatus(option.value)}
              role="menuitemradio"
              aria-checked={option.value === currentUserStatus}
            >
              <span class={`status ${option.value}`}></span>
              <span class="status-option-label">{option.label}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
      <div class="connection-control">
        <button
          class="connection-info"
          bind:this={statsMenuButton}
          aria-haspopup="true"
          aria-expanded={statsMenuOpen}
          on:click={toggleStatsMenu}
          title="Connection stats"
        >
          <PingDot ping={$ping} />
          <ConnectionBars strength={serverStrength} />
        </button>
        {#if statsMenuOpen}
          <div
            class="stats-menu"
            bind:this={statsMenuElement}
            role="menu"
            tabindex="-1"
            on:click|stopPropagation
            on:keydown={handleStatsMenuKeydown}
          >
            <ConnectionStatsPanel />
          </div>
        {/if}
      </div>
    </div>
    <div class="action-group">
    <button
      class="icon-btn"
      on:click={() => theme.toggle()}
      title={$theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
      aria-pressed={$theme === 'light'}
    >
      {#if $theme === 'dark'}
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2" />
          <path d="M12 20v2" />
          <path d="m4.93 4.93 1.41 1.41" />
          <path d="m17.66 17.66 1.41 1.41" />
          <path d="M2 12h2" />
          <path d="M20 12h2" />
          <path d="m6.34 17.66-1.41 1.41" />
          <path d="m19.07 4.93-1.41 1.41" />
        </svg>
        <span class="sr-only">Switch to light theme</span>
      {:else}
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401" />
        </svg>
        <span class="sr-only">Switch to dark theme</span>
      {/if}
    </button>
    <button class="icon-btn" on:click={onEditTopic} title="Edit channel topic">
      <svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path
          d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"
        />
        <path d="m15 5 4 4" />
      </svg>
      <span class="sr-only">Edit channel topic</span>
    </button>
    <div class="notification-control">
      <button
        class="icon-btn"
        bind:this={notificationMenuButton}
        aria-haspopup="true"
        aria-expanded={notificationMenuOpen}
        on:click={toggleNotificationMenu}
        title={`Channel notifications: ${notificationMenuLabel}`}
      >
        <span class="notification-icon" aria-hidden="true">
          {#if currentNotificationPreference === 'mute'}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M8.7 3A6 6 0 0 1 18 8c0 4.5 1.8 5.3 2 6"/><path d="M17 17H4c-.3 0-.6-.1-.7-.4-.2-.2-.3-.5-.2-.8.5-1 1.9-2.7 1.9-7.8"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/><line x1="2" y1="2" x2="22" y2="22"/></svg>
          {:else if currentNotificationPreference === 'mentions'}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8"/></svg>
          {:else}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/></svg>
          {/if}
        </span>
        <span class="sr-only">Configure channel notifications</span>
      </button>
      {#if notificationMenuOpen}
        <div
          class="notification-menu"
          bind:this={notificationMenuElement}
          role="menu"
          tabindex="-1"
          on:click|stopPropagation
          on:keydown={handleNotificationMenuKeydown}
        >
          {#each NOTIFICATION_OPTIONS as option}
            <button
              class:active={option.value === currentNotificationPreference}
              on:click={() => selectNotificationPreference(option.value)}
              role="menuitemradio"
              aria-checked={option.value === currentNotificationPreference}
            >
              <span class="notification-option-icon" aria-hidden="true">{option.icon}</span>
              <span class="notification-option-text">
                <span class="label">{option.label}</span>
                <span class="description">{option.description}</span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
    </div>
    <div class="action-group">
    <button class="icon-btn" on:click={onOpenSearch} title="Search messages">
      <svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="7" />
        <line x1="20" y1="20" x2="16.65" y2="16.65" />
      </svg>
      <span class="sr-only">Search messages</span>
    </button>
    {#if showServerDashboard}
      <button class="icon-btn" on:click={onOpenServerDashboard} title="Server dashboard">
        <svg
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10" />
          <path d="M9.5 12l1.8 1.8 3.2-3.6" />
        </svg>
        <span class="sr-only">Open server dashboard</span>
      </button>
    {/if}
    <button class="icon-btn" on:click={onOpenSettings} title="Settings">
      <svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path
          d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"
        />
        <circle cx="12" cy="12" r="3" />
      </svg>
      <span class="sr-only">Open settings</span>
    </button>
    <button class="icon-btn" on:click={onLeaveServer} title="Leave Server">
      <svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="m16 17 5-5-5-5" />
        <path d="M21 12H9" />
        <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
      </svg>
      <span class="sr-only">Leave server</span>
    </button>
    </div>
    <div class="action-group">
    <button class="icon-btn danger" on:click={onLogout} title="Logout">
      <svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M12 2v10" />
        <path d="M18.4 6.6a9 9 0 1 1-12.77.04" />
      </svg>
      <span class="sr-only">Sign out</span>
    </button>
    </div>
  </div>
</div>

<style>
  /* Compact toolbar: channel identity left, grouped actions right. */
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-4);
    min-height: 3.5rem;
    padding: var(--space-2) var(--space-4);
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-surface-outline);
    flex-wrap: wrap;
  }

  .title {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    min-width: 0;
  }

  .title h1 {
    font-size: var(--text-lg);
    font-weight: 600;
    white-space: nowrap;
  }

  .title h1::before {
    content: '#';
    margin-right: var(--space-1);
    color: var(--color-muted);
    font-weight: 500;
  }

  .topic {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--color-muted);
    max-width: min(32rem, 40vw);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-left: 1px solid var(--color-surface-outline);
    padding-left: var(--space-3);
  }

  .topic.empty {
    font-style: italic;
    opacity: 0.8;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-wrap: wrap;
  }

  .action-group {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    position: relative;
  }

  .action-group + .action-group {
    margin-left: var(--space-2);
    padding-left: var(--space-2);
    border-left: 1px solid var(--color-surface-outline);
  }

  .user {
    display: inline-flex;
    align-items: center;
    padding: 0 var(--space-2);
    color: var(--color-on-surface-variant);
    font-weight: 600;
    font-size: var(--text-sm);
  }

  .status-control {
    position: relative;
  }

  .status-button {
    min-height: var(--control-height);
    padding: 0 var(--space-3);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .status-button-label {
    text-transform: capitalize;
  }

  .status-menu {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    min-width: 11rem;
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    padding: var(--space-1);
    display: flex;
    flex-direction: column;
    z-index: var(--z-dropdown);
  }

  .status-menu button {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    border: none;
    background: transparent;
    color: inherit;
    border-radius: var(--radius-xs);
    padding: var(--space-2) var(--space-3);
    text-align: left;
    font-size: var(--text-md);
    cursor: pointer;
  }

  .status-menu button:hover,
  .status-menu button:focus-visible,
  .status-menu button.active {
    background: var(--color-surface-raised);
    outline: none;
  }

  .status-menu .status {
    width: 0.5rem;
    height: 0.5rem;
  }

  .status-menu button.active {
    font-weight: 600;
  }

  .status-option-label {
    flex: 1;
    text-transform: capitalize;
  }

  .connection-control {
    position: relative;
  }

  .connection-info {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border: none;
    background: transparent;
    border-radius: var(--radius-xs);
    cursor: pointer;
  }

  .connection-info:hover,
  .connection-info:focus-visible {
    background: var(--color-surface-raised);
    outline: none;
  }

  .stats-menu {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    z-index: var(--z-dropdown);
  }

  .notification-control {
    position: relative;
  }

  .notification-icon {
    display: inline-flex;
    line-height: 1;
  }

  .notification-menu {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    min-width: 16rem;
    padding: var(--space-1);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
    box-shadow: var(--shadow-md);
    display: flex;
    flex-direction: column;
    z-index: var(--z-dropdown);
  }

  .notification-menu button {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-xs);
    border: none;
    background: transparent;
    color: var(--color-on-surface);
    cursor: pointer;
    text-align: left;
  }

  .notification-menu button:hover,
  .notification-menu button.active {
    background: var(--color-surface-raised);
  }

  .notification-option-icon {
    font-size: var(--text-md);
  }

  .notification-option-text {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }

  .notification-option-text .label {
    font-weight: 500;
    font-size: var(--text-md);
  }

  .notification-option-text .description {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .status {
    width: 0.625rem;
    height: 0.625rem;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status.online {
    background: var(--color-success);
  }

  .status.away {
    background: var(--color-warning);
  }

  .status.busy {
    background: var(--color-error);
  }

  .status.offline {
    background: var(--color-outline-strong);
  }

  @media (max-width: 768px) {
    .header {
      flex-direction: column;
      align-items: stretch;
    }

    .actions {
      justify-content: flex-start;
    }

    .topic {
      max-width: 100%;
    }
  }
</style>
