<!-- Right-hand sidebar listing online/offline users with status and roles.
     Clicking a user opens a direct message conversation; right-clicking opens
     the user menu (DM, roles, moderation). -->
<script lang="ts">
  import { onlineUsers } from '$lib/stores/online';
  import { offlineUsers } from '$lib/stores/users';
  import { roles } from '$lib/stores/roles';
  import { session } from '$lib/stores/session';
  import { dm } from '$lib/stores/dm';
  import { rightSidebarWidth } from '$lib/stores/layout';
  import { STATUS_LABELS } from '$lib/stores/status';
  import { ensureStatus } from '$lib/chat/helpers';
  import type { UserStatus } from '$lib/types';

  export let statusMap: Record<string, UserStatus>;
  export let onUserContextMenu: (event: MouseEvent, user: string) => void;
  export let onOpenDm: (user: string) => void;

  const dmUnread = dm.unread;

  function handleClick(user: string) {
    if (user === $session.user) return;
    onOpenDm(user);
  }
</script>

<div class="sidebar" style="width: {$rightSidebarWidth}px">
  <h2>Users</h2>
  <h3>Online</h3>
  <ul>
    {#each $onlineUsers as user}
      <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
      <!-- svelte-ignore a11y-click-events-have-key-events -->
      <li
        class:clickable={user !== $session.user}
        on:click={() => handleClick(user)}
        on:contextmenu={(e) => onUserContextMenu(e, user)}
      >
        <span class={`status ${ensureStatus(statusMap, user, 'online')}`}></span>
        <span class="status-label">{STATUS_LABELS[ensureStatus(statusMap, user, 'online')]}</span>
        <span
          class="username"
          style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
          >{user}</span
        >
        {#if $dmUnread[user]}
          <span class="dm-badge" title="Unread direct messages">{$dmUnread[user]}</span>
        {/if}
        {#if $roles[user]}
          <span
            class="role"
            style={$roles[user].color ? `color: ${$roles[user].color}` : ''}
            >{$roles[user].role}</span
          >
        {/if}
      </li>
    {/each}
  </ul>
  <h3>Offline</h3>
  <ul>
    {#each $offlineUsers as user}
      <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
      <!-- svelte-ignore a11y-click-events-have-key-events -->
      <li
        class:clickable={user !== $session.user}
        on:click={() => handleClick(user)}
        on:contextmenu={(e) => onUserContextMenu(e, user)}
      >
        <span class={`status ${ensureStatus(statusMap, user)}`}></span>
        <span class="status-label">{STATUS_LABELS[ensureStatus(statusMap, user)]}</span>
        <span
          class="username"
          style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
          >{user}</span
        >
        {#if $dmUnread[user]}
          <span class="dm-badge" title="Unread direct messages">{$dmUnread[user]}</span>
        {/if}
        {#if $roles[user]}
          <span
            class="role"
            style={$roles[user].color ? `color: ${$roles[user].color}` : ''}
            >{$roles[user].role}</span
          >
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .sidebar {
    width: clamp(240px, 18vw, 280px);
    background: color-mix(in srgb, var(--color-surface-elevated) 84%, transparent);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-xs);
    padding: clamp(1rem, 2vw, 1.3rem);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    min-width: 0;
    overflow-y: auto;
  }

  .sidebar h2 {
    margin: 0;
    font-size: var(--text-lg);
  }

  .sidebar h3 {
    margin: 0;
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--color-muted);
    font-weight: 700;
  }

  .sidebar ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .sidebar li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.5rem;
    border-radius: var(--radius-sm);
    font-size: var(--text-md);
  }

  .sidebar li:hover {
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
  }

  .sidebar li.clickable {
    cursor: pointer;
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

  .status-label {
    font-size: var(--text-sm);
    color: var(--color-muted);
    text-transform: capitalize;
    min-width: 3.5rem;
  }

  .dm-badge {
    background: var(--color-error);
    color: #fff;
    border-radius: 999px;
    font-size: var(--text-xs);
    font-weight: 700;
    min-width: 1.2rem;
    padding: 0.05rem 0.35rem;
    text-align: center;
    flex-shrink: 0;
  }
</style>
