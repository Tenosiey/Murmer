<!--
  Chat page header: channel name/topic, own status menu, connection info,
  theme/focus/notification toggles and the search/settings/leave/logout actions.
-->
<script lang="ts">
  import PingDot from '$lib/components/PingDot.svelte';
  import ConnectionBars from '$lib/components/ConnectionBars.svelte';
  import { ping } from '$lib/stores/ping';
  import { session } from '$lib/stores/session';
  import { theme } from '$lib/stores/theme';
  import { focusMode } from '$lib/stores/layout';
  import { statuses, STATUS_LABELS, STATUS_EMOJIS, USER_STATUS_VALUES } from '$lib/stores/status';
  import { channelNotifications, type ChannelNotificationPreference } from '$lib/stores/channelNotifications';
  import { NOTIFICATION_OPTIONS } from '$lib/chat/constants';
  import { ensureStatus, notificationButtonIcon } from '$lib/chat/helpers';
  import type { UserStatus } from '$lib/types';

  export let channelId: number;
  export let channelName: string;
  export let topic: string;
  export let serverStrength: number;
  export let statusMap: Record<string, UserStatus>;
  export let onEditTopic: () => void;
  export let onOpenSearch: () => void;
  export let onOpenSettings: () => void;
  export let onLeaveServer: () => void;
  export let onLogout: () => void;

  const statusOptions: Array<{ value: UserStatus; label: string; emoji: string }> =
    USER_STATUS_VALUES.map((value) => ({
      value,
      label: STATUS_LABELS[value],
      emoji: STATUS_EMOJIS[value]
    }));

  let statusMenuOpen = false;
  let statusMenuButton: HTMLButtonElement | null = null;
  let statusMenuElement: HTMLDivElement | null = null;

  let notificationMenuOpen = false;
  let notificationMenuButton: HTMLButtonElement | null = null;
  let notificationMenuElement: HTMLDivElement | null = null;

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
  }

  function toggleStatusMenu(event: MouseEvent) {
    event.stopPropagation();
    if (notificationMenuOpen) {
      notificationMenuOpen = false;
    }
    statusMenuOpen = !statusMenuOpen;
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

  function toggleFocusMode() {
    focusMode.update((v) => !v);
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
        class="action-button status-button"
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
              <span class="status-option-emoji" aria-hidden="true">{option.emoji}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
      <div class="connection-info">
        <PingDot ping={$ping} />
        <ConnectionBars strength={serverStrength} />
      </div>
    </div>
    <div class="action-group">
    <button
      class="action-button"
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
    <button class="action-button" on:click={onEditTopic} title="Edit channel topic">
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
    <button
      class="action-button focus-toggle"
      class:focusActive={$focusMode}
      aria-pressed={$focusMode}
      on:click={toggleFocusMode}
      title={$focusMode ? 'Exit focus mode' : 'Enter focus mode'}
    >
      {#if $focusMode}
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
          <path d="M15 3h6v6" />
          <path d="m21 3-7 7" />
          <path d="m3 21 7-7" />
          <path d="M9 21H3v-6" />
        </svg>
        <span>Restore</span>
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
          <circle cx="12" cy="12" r="3" />
          <path d="M3 7V5a2 2 0 0 1 2-2h2" />
          <path d="M17 3h2a2 2 0 0 1 2 2v2" />
          <path d="M21 17v2a2 2 0 0 1-2 2h-2" />
          <path d="M7 21H5a2 2 0 0 1-2-2v-2" />
        </svg>
        <span>Focus</span>
      {/if}
    </button>
    <div class="notification-control">
      <button
        class="action-button"
        bind:this={notificationMenuButton}
        aria-haspopup="true"
        aria-expanded={notificationMenuOpen}
        on:click={toggleNotificationMenu}
        title={`Channel notifications: ${notificationMenuLabel}`}
      >
        <span class="notification-icon">{notificationButtonIcon(currentNotificationPreference)}</span>
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
    <button class="action-button" on:click={onOpenSearch} title="Search messages">
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
    <button class="action-button" on:click={onOpenSettings} title="Settings">
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
    <button class="action-button" on:click={onLeaveServer} title="Leave Server">
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
    <button class="action-button danger" on:click={onLogout} title="Logout">
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
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: clamp(1rem, 2vw, 1.35rem) clamp(1rem, 3vw, 1.5rem);
    border-radius: var(--radius-lg);
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--color-primary) 18%, var(--color-surface-raised)),
      color-mix(in srgb, var(--color-tertiary) 12%, var(--color-surface-raised))
    );
    border: 1px solid color-mix(in srgb, var(--color-primary) 20%, transparent);
    box-shadow: var(--shadow-sm);
    gap: 1rem;
    flex-wrap: wrap;
  }

  .title {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    min-width: 0;
  }

  .title h1 {
    margin: 0;
    font-size: clamp(1.25rem, 2.5vw, 1.7rem);
    letter-spacing: -0.01em;
  }

  .topic {
    margin: 0;
    font-size: var(--text-md);
    color: color-mix(in srgb, var(--color-on-primary) 82%, transparent);
    max-width: min(40rem, 60vw);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .topic.empty {
    font-style: italic;
    opacity: 0.7;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
  }

  .action-group {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding-inline: 0.2rem;
    position: relative;
  }

  .action-group + .action-group::before {
    content: '';
    position: absolute;
    left: -0.1rem;
    top: 20%;
    bottom: 20%;
    width: 1px;
    background: color-mix(in srgb, var(--color-on-surface) 12%, transparent);
    border-radius: var(--radius-pill);
  }

  .user {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.9rem;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--color-on-primary) 15%, transparent);
    color: var(--color-on-primary);
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .user::before {
    content: '🧑‍🚀';
  }

  .status-control {
    position: relative;
  }

  .status-button {
    width: auto;
    height: auto;
    min-width: 0;
    padding: 0.4rem 0.85rem;
    gap: 0.45rem;
    font-weight: 600;
    font-size: var(--text-sm);
    justify-content: flex-start;
    white-space: nowrap;
  }

  .status-button svg {
    margin-left: 0.35rem;
  }

  .status-button-label {
    text-transform: capitalize;
  }

  .status-menu {
    position: absolute;
    top: calc(100% + 0.4rem);
    right: 0;
    min-width: 12rem;
    background: color-mix(in srgb, var(--color-surface-elevated) 95%, transparent);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg);
    padding: 0.35rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    z-index: var(--z-dropdown);
  }

  .status-menu button {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    border: none;
    background: transparent;
    color: inherit;
    border-radius: var(--radius-sm);
    padding: 0.45rem 0.6rem;
    text-align: left;
    font-size: var(--text-md);
    cursor: pointer;
  }

  .status-menu button:hover,
  .status-menu button:focus-visible,
  .status-menu button.active {
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
    outline: none;
  }

  .status-menu .status {
    width: 0.6rem;
    height: 0.6rem;
  }

  .status-menu button.active {
    font-weight: 600;
  }

  .status-option-label {
    flex: 1;
    text-transform: capitalize;
  }

  .status-option-emoji {
    font-size: 1rem;
  }

  .connection-info {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.4rem 0.8rem;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--color-on-primary) 12%, transparent);
  }

  .action-button {
    min-width: 2.5rem;
    width: auto;
    min-height: 2.5rem;
    height: auto;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-outline-strong) 70%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 82%, transparent);
    color: color-mix(in srgb, var(--color-on-surface) 92%, var(--color-muted) 8%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    padding: 0.55rem;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
  }

  .action-button svg {
    width: 1.1rem;
    height: 1.1rem;
  }

  .action-button:hover {
    border-color: color-mix(in srgb, var(--color-outline-strong) 90%, transparent);
    transform: translateY(-1px);
    box-shadow: var(--shadow-xs);
  }

  .action-button.danger {
    color: var(--color-error);
    border-color: color-mix(in srgb, var(--color-error) 40%, transparent);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .action-button.danger:hover {
    border-color: color-mix(in srgb, var(--color-error) 55%, transparent);
    background: color-mix(in srgb, var(--color-error) 18%, transparent);
  }

  .action-button.focus-toggle {
    padding-inline: 0.9rem;
    width: auto;
    font-size: var(--text-md);
  }

  .action-button.focus-toggle svg {
    width: 1.05rem;
    height: 1.05rem;
  }

  .action-button.focus-toggle span {
    font-weight: 600;
    font-size: var(--text-sm);
  }

  .action-button.focus-toggle.focusActive {
    background: color-mix(in srgb, var(--color-secondary) 20%, var(--color-surface-elevated) 80%);
    color: var(--color-on-surface);
  }

  .notification-control {
    position: relative;
  }

  .notification-icon {
    font-size: var(--text-lg);
    line-height: 1;
  }

  .notification-menu {
    position: absolute;
    top: calc(100% + 0.4rem);
    right: 0;
    min-width: 16rem;
    padding: 0.5rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 16%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 95%, transparent);
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    z-index: var(--z-dropdown);
  }

  .notification-menu button {
    display: flex;
    gap: 0.65rem;
    align-items: center;
    padding: 0.5rem 0.65rem;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    color: var(--color-on-surface);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition), transform var(--transition);
  }

  .notification-menu button:hover,
  .notification-menu button.active {
    background: color-mix(in srgb, var(--color-primary) 15%, transparent);
    transform: translateY(-1px);
  }

  .notification-option-icon {
    font-size: 1rem;
  }

  .notification-option-text {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .notification-option-text .label {
    font-weight: 600;
  }

  .notification-option-text .description {
    font-size: var(--text-sm);
    color: color-mix(in srgb, var(--color-on-surface) 70%, transparent);
  }

  .status {
    width: 0.75rem;
    height: 0.75rem;
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
    background: color-mix(in srgb, var(--color-muted) 40%, transparent);
  }

  @media (max-width: 768px) {
    .header {
      flex-direction: column;
      align-items: stretch;
    }

    .actions {
      justify-content: flex-start;
    }
  }
</style>
