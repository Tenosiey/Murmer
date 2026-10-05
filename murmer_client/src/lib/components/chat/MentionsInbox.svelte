<!--
  Mentions inbox: recent messages that mention you, across every channel —
  by name, or through a role you hold. Opening it asks the server again
  (`load-mentions`) and clears the unseen dot; the list itself is
  `stores/mentionInbox.ts`, which also collects mentions arriving live.
-->
<script lang="ts">
  import { chat } from '$lib/stores/chat';
  import { mentionInbox } from '$lib/stores/mentionInbox';
  import { channels } from '$lib/stores/channels';
  import { displayNames } from '$lib/stores/profiles';
  import { formatLocalDateTime } from '$lib/chat/helpers';
  import { modalFocus } from '$lib/focus';
  import UserAvatar from '$lib/components/UserAvatar.svelte';

  interface Props {
    open: boolean;
    close: () => void;
    /** Jump to a message, switching channels if needed. */
    onOpenMessage: (channelId: number, messageId: number) => void;
  }

  let { open, close, onOpenMessage }: Props = $props();

  $effect(() => {
    if (!open) return;
    chat.loadMentions();
    mentionInbox.markSeen();
  });

  let entries = $derived($mentionInbox.entries);

  function channelName(id: number): string {
    return $channels.find((channel) => channel.id === id)?.name ?? 'a channel you cannot see';
  }

  function sealed(channelId: number): boolean {
    return $channels.find((channel) => channel.id === channelId)?.e2ee === true;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') close();
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    // Keyboard activation of the focused backdrop only; keystrokes inside the
    // modal content bubble here and must not close it.
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') close();
  }

  function jump(channelId: number, messageId: number) {
    onOpenMessage(channelId, messageId);
    close();
  }
</script>

{#if open}
  <div
    class="modal-overlay"
    onclick={close}
    onkeydown={handleOverlayKeydown}
    role="dialog"
    aria-modal="true"
    aria-labelledby="mentions-inbox-title"
    tabindex="-1"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      use:modalFocus
      class="modal-content"
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleKeydown}
      role="document"
      tabindex="0"
    >
      <div class="modal-header">
        <h2 id="mentions-inbox-title">Mentions</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label="Close mentions">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="modal-body">
        {#if entries.length === 0}
          <p class="empty">
            {$mentionInbox.loading ? 'Looking for mentions…' : 'Nobody has mentioned you recently.'}
          </p>
        {:else}
          <ul>
            {#each entries as entry (entry.id)}
              <li>
                <button type="button" class="mention" onclick={() => jump(entry.channelId, entry.id)}>
                  <UserAvatar name={entry.user} size="sm" />
                  <span class="mention-main">
                    <span class="mention-meta">
                      <strong>{$displayNames(entry.user)}</strong>
                      <span>#{channelName(entry.channelId)}</span>
                      {#if entry.timestamp}
                        <span>· {formatLocalDateTime(entry.timestamp)}</span>
                      {/if}
                    </span>
                    <span class="mention-text">
                      {#if entry.text !== null}
                        {entry.text}
                      {:else if sealed(entry.channelId)}
                        <em>Encrypted — waiting for this channel’s key.</em>
                      {:else}
                        <em>A file or image.</em>
                      {/if}
                    </span>
                  </span>
                </button>
              </li>
            {/each}
          </ul>
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
    max-height: min(680px, 86vh);
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
    padding: var(--space-3);
    overflow-y: auto;
  }

  .empty {
    margin: 0;
    padding: var(--space-3);
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .mention {
    width: 100%;
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: none;
    border-radius: var(--radius-md);
    background: none;
    color: var(--color-on-surface);
    text-align: left;
    cursor: pointer;
  }

  .mention:hover,
  .mention:focus-visible {
    background: var(--color-surface-raised);
  }

  .mention-main {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .mention-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .mention-meta strong {
    color: var(--color-on-surface);
  }

  /* Long messages are a preview here; the jump shows the whole thing. */
  .mention-text {
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
</style>
