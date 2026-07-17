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
  import UserAvatar from '$lib/components/UserAvatar.svelte';
  import type { UserStatus } from '$lib/types';

  interface Props {
    statusMap: Record<string, UserStatus>;
    onUserContextMenu: (event: MouseEvent, user: string) => void;
    onOpenDm: (user: string) => void;
  }

  let { statusMap, onUserContextMenu, onOpenDm }: Props = $props();

  const dmUnread = dm.unread;

  function handleClick(user: string) {
    if (user === $session.user) return;
    onOpenDm(user);
  }
</script>

<div class="sidebar" style="width: {$rightSidebarWidth}px">
  <h3>Online — {$onlineUsers.length}</h3>
  <ul>
    {#each $onlineUsers as user}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li
        class:clickable={user !== $session.user}
        title={STATUS_LABELS[ensureStatus(statusMap, user, 'online')]}
        onclick={() => handleClick(user)}
        oncontextmenu={(e) => onUserContextMenu(e, user)}
      >
        <span class="avatar-wrap">
          <UserAvatar name={user} size="sm" />
          <span class={`status ${ensureStatus(statusMap, user, 'online')}`}></span>
        </span>
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
  <h3>Offline — {$offlineUsers.length}</h3>
  <ul>
    {#each $offlineUsers as user}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li
        class="offline-user"
        class:clickable={user !== $session.user}
        title={STATUS_LABELS[ensureStatus(statusMap, user)]}
        onclick={() => handleClick(user)}
        oncontextmenu={(e) => onUserContextMenu(e, user)}
      >
        <span class="avatar-wrap">
          <UserAvatar name={user} size="sm" />
          <span class={`status ${ensureStatus(statusMap, user)}`}></span>
        </span>
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
  /* Right pane: sits on the app background, no card chrome. */
  .sidebar {
    width: clamp(200px, 16vw, 260px);
    flex-shrink: 0;
    background: var(--color-bg);
    padding: var(--space-3) var(--space-2);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    overflow-y: auto;
  }

  .sidebar h3 {
    margin: var(--space-2) var(--space-2) var(--space-1);
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-muted);
    font-weight: 600;
  }

  .sidebar h3:first-child {
    margin-top: 0;
  }

  .sidebar ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .sidebar li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 2rem;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    font-size: var(--text-md);
  }

  .sidebar li:hover {
    background: var(--color-surface-elevated);
  }

  .sidebar li.clickable {
    cursor: pointer;
  }

  .offline-user {
    opacity: 0.55;
  }

  /* The status dot sits on the avatar's corner, ringed by the background so
     it stays readable on top of avatar images. */
  .avatar-wrap {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
  }

  .status {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    box-shadow: 0 0 0 2px var(--color-bg);
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

  .username {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-on-surface-variant);
    font-weight: 500;
  }

  .role {
    font-size: var(--text-xs);
    font-weight: 500;
    color: var(--color-muted);
    flex-shrink: 0;
  }

  .dm-badge {
    background: var(--color-error);
    color: var(--color-surface);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    min-width: 1.125rem;
    line-height: 1.125rem;
    padding: 0 var(--space-1);
    text-align: center;
    flex-shrink: 0;
  }
</style>
