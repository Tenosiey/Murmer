<!-- Right-hand sidebar listing online/offline users with status and roles. -->
<script lang="ts">
  import { onlineUsers } from '$lib/stores/online';
  import { offlineUsers } from '$lib/stores/users';
  import { roles } from '$lib/stores/roles';
  import { session } from '$lib/stores/session';
  import { rightSidebarWidth } from '$lib/stores/layout';
  import { STATUS_LABELS } from '$lib/stores/status';
  import { ensureStatus } from '$lib/chat/helpers';
  import type { UserStatus } from '$lib/types';

  export let statusMap: Record<string, UserStatus>;
  export let currentUserIsOwner: boolean;
  export let onUserContextMenu: (event: MouseEvent, user: string) => void;
</script>

<div class="sidebar" style="width: {$rightSidebarWidth}px">
  <h2>Users</h2>
  <h3>Online</h3>
  <ul>
    {#each $onlineUsers as user}
      <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
      <li
        class:clickable={currentUserIsOwner && user !== $session.user}
        on:contextmenu={(e) => onUserContextMenu(e, user)}
      >
        <span class={`status ${ensureStatus(statusMap, user, 'online')}`}></span>
        <span class="status-label">{STATUS_LABELS[ensureStatus(statusMap, user, 'online')]}</span>
        <span
          class="username"
          style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
          >{user}</span
        >
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
      <li
        class:clickable={currentUserIsOwner && user !== $session.user}
        on:contextmenu={(e) => onUserContextMenu(e, user)}
      >
        <span class={`status ${ensureStatus(statusMap, user)}`}></span>
        <span class="status-label">{STATUS_LABELS[ensureStatus(statusMap, user)]}</span>
        <span
          class="username"
          style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
          >{user}</span
        >
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
    cursor: context-menu;
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
</style>
