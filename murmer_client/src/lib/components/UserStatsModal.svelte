<!--
  Lifetime stats of another member, opened from the user context menu.
  The server only reveals stats of users who are currently opted in; a
  denial arrives as a `stats-not-available` error frame shown inline.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { stats, statsSnapshot } from '$lib/stores/stats';
  import UserStatsPanel from '$lib/components/UserStatsPanel.svelte';
  import type { Message } from '$lib/types';

  export let open: boolean;
  export let user: string | null;
  export let close: () => void;

  let unavailable = false;
  let requestedFor: string | null = null;

  $: if (open && user && requestedFor !== user) {
    requestedFor = user;
    unavailable = false;
    stats.fetchStats(user);
  }
  $: if (!open) {
    requestedFor = null;
    unavailable = false;
  }

  $: snapshot = $statsSnapshot && user && $statsSnapshot.user === user ? $statsSnapshot : null;

  function handleServerError(msg: Message) {
    if (open && (msg as any).message === 'stats-not-available') {
      unavailable = true;
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      close();
    }
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      close();
    }
  }
</script>

{#if open && user}
  <div class="modal-overlay" on:click={close} on:keydown={handleOverlayKeydown} role="dialog" aria-modal="true" aria-labelledby="user-stats-title" tabindex="-1">
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
    <div class="modal-content" on:click|stopPropagation on:keydown={handleKeydown} role="document" tabindex="0">
      <div class="modal-header">
        <h2 id="user-stats-title">Stats — {user}</h2>
        <button class="icon-btn close-btn" on:click={close} aria-label="Close stats">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="modal-body">
        {#if unavailable}
          <p class="unavailable">
            {user}'s stats are not available. They may have opted out of stat tracking, or
            tracking is disabled on this server.
          </p>
        {:else if snapshot}
          <UserStatsPanel {snapshot} />
        {:else}
          <p class="unavailable">Loading stats…</p>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    z-index: var(--z-modal);
  }

  .modal-content {
    background: var(--color-surface-elevated);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    border: 1px solid var(--color-surface-outline);
    width: min(640px, 94vw);
    max-height: min(640px, 86vh);
    overflow: hidden;
    display: flex;
    flex-direction: column;
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
    min-height: 0;
    padding: var(--space-5);
    overflow-y: auto;
  }

  .unavailable {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-md);
  }
</style>
