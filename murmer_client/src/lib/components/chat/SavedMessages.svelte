<!--
  Saved messages: the reader's own bookmarks on this server, newest save
  first. The list is `stores/savedMessages.ts`, kept on this device only;
  an entry from an encrypted channel holds no text, so it reads as a pointer.
-->
<script lang="ts">
  import { savedMessages, toggleSavedMessage, type SavedMessage } from '$lib/stores/savedMessages';
  import { channels } from '$lib/stores/channels';
  import { displayNames } from '$lib/stores/profiles';
  import { formatLocalDateTime } from '$lib/chat/helpers';
  import { modalFocus } from '$lib/focus';
  import UserAvatar from '$lib/components/UserAvatar.svelte';
  import { t } from '$lib/i18n';

  interface Props {
    open: boolean;
    close: () => void;
    /** Jump to a message, switching channels if needed. */
    onOpenMessage: (channelId: number, messageId: number) => void;
  }

  let { open, close, onOpenMessage }: Props = $props();

  let entries = $derived($savedMessages);

  function channelName(id: number): string {
    return $channels.find((channel) => channel.id === id)?.name ?? t('mentionsInbox.hiddenChannel');
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

  function remove(entry: SavedMessage) {
    toggleSavedMessage(entry);
  }
</script>

{#if open}
  <div
    class="modal-overlay"
    onclick={close}
    onkeydown={handleOverlayKeydown}
    role="dialog"
    aria-modal="true"
    aria-labelledby="saved-messages-title"
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
        <h2 id="saved-messages-title">{t('savedMessages.title')}</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label={t('savedMessages.close')}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="modal-body">
        {#if entries.length === 0}
          <p class="empty">{t('savedMessages.empty')}</p>
        {:else}
          <ul>
            {#each entries as entry (entry.id)}
              <li class="saved-row">
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
                        <em>{t('savedMessages.encrypted')}</em>
                      {:else}
                        <em>{t('mentionsInbox.aFileOrImage')}</em>
                      {/if}
                    </span>
                  </span>
                </button>
                <button
                  type="button"
                  class="icon-btn remove-btn"
                  onclick={() => remove(entry)}
                  title={t('savedMessages.remove')}
                >
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                    <line x1="18" y1="6" x2="6" y2="18"></line>
                    <line x1="6" y1="6" x2="18" y2="18"></line>
                  </svg>
                  <span class="sr-only">{t('savedMessages.remove')}</span>
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

  .saved-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-1);
  }

  .remove-btn {
    flex-shrink: 0;
  }

  .mention {
    flex: 1;
    min-width: 0;
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
