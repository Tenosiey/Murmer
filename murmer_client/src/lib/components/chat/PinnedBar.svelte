<!-- Bar above the message list showing pinned messages for the current channel. -->
<script lang="ts">
  import { chat } from '$lib/stores/chat';
  import type { PinnedEntry } from '$lib/stores/pins';
  import type { Message } from '$lib/types';
  import { PIN_PREVIEW_LIMIT } from '$lib/chat/constants';
  import { formatLocalDateTime } from '$lib/chat/helpers';
  import { hideSpoilers } from '$lib/spoilers';
  import { t } from '$lib/i18n';

  
  interface Props {
    entries: PinnedEntry[];
    /** Messages of the current channel, used to resolve fresh content for a pin. */
    messages: Message[];
    onFocusMessage: (messageId: number) => void;
  }

  let { entries, messages, onFocusMessage }: Props = $props();

  function resolvePinnedMessage(entry: PinnedEntry): Message | undefined {
    return messages.find((message) => message.id === entry.id);
  }

  function formatPinnedPreview(entry: PinnedEntry): string {
    const message = resolvePinnedMessage(entry);
    const base = message?.text ?? entry.text ?? '';
    const trimmed = hideSpoilers(base.trim());
    if (trimmed.length > 0) {
      return trimmed.length > PIN_PREVIEW_LIMIT ? `${trimmed.slice(0, PIN_PREVIEW_LIMIT)}…` : trimmed;
    }
    if (message?.image || entry.image) {
      return t('pinnedBar.image');
    }
    return t('pinnedBar.message');
  }

  function pinnedAuthor(entry: PinnedEntry): string {
    const message = resolvePinnedMessage(entry);
    return message?.user ?? entry.user ?? t('pinnedBar.unknownAuthor');
  }

  function pinnedTimestamp(entry: PinnedEntry): string {
    const source = resolvePinnedMessage(entry)?.timestamp ?? entry.timestamp ?? entry.pinnedAt;
    return formatLocalDateTime(source) ?? '';
  }
</script>

{#if entries.length > 0}
  <div class="pinned-bar" role="region" aria-label={t('pinnedBar.pinnedMessages')}>
    <div class="pinned-header">
      <span class="pinned-title">{t('pinnedBar.pinned')}</span>
      <span class="pinned-count">{entries.length}</span>
    </div>
    <ul class="pinned-list">
      {#each entries as entry (entry.id)}
        <li class="pinned-item">
          <button class="pinned-preview" onclick={() => onFocusMessage(entry.id)}>
            <span class="pinned-author">{pinnedAuthor(entry)}</span>
            <span class="pinned-text">{formatPinnedPreview(entry)}</span>
            {#if pinnedTimestamp(entry)}
              <span class="pinned-timestamp">{pinnedTimestamp(entry)}</span>
            {/if}
          </button>
          <button
            class="pinned-remove"
            onclick={() => chat.sendRaw({ type: 'unpin-message', messageId: entry.id })}
            aria-label={t('pinnedBar.unpinMessage')}
          >
            ✕
          </button>
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .pinned-bar {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
  }

  .pinned-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .pinned-title {
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-muted);
  }

  .pinned-count {
    font-size: var(--text-xs);
    font-weight: 600;
    min-width: 1.125rem;
    line-height: 1.125rem;
    padding: 0 var(--space-1);
    border-radius: var(--radius-pill);
    text-align: center;
    background: var(--color-surface-raised);
    color: var(--color-on-surface-variant);
  }

  .pinned-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    max-height: 10rem;
    overflow-y: auto;
  }

  .pinned-item {
    display: flex;
    gap: var(--space-1);
    align-items: center;
  }

  .pinned-preview {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    text-align: left;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    padding: var(--space-1) var(--space-2);
    color: var(--color-on-surface-variant);
    cursor: pointer;
  }

  .pinned-preview:hover {
    background: var(--color-surface-raised);
  }

  .pinned-author {
    font-weight: 600;
    font-size: var(--text-sm);
    color: var(--color-on-surface);
    flex-shrink: 0;
  }

  .pinned-text {
    font-size: var(--text-sm);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pinned-timestamp {
    font-size: var(--text-xs);
    color: var(--color-muted);
    font-family: var(--font-mono);
    flex-shrink: 0;
    margin-left: auto;
  }

  .pinned-remove {
    border: none;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-muted);
    width: 1.5rem;
    height: 1.5rem;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font-size: var(--text-xs);
  }

  .pinned-remove:hover {
    background: color-mix(in srgb, var(--color-error) 14%, transparent);
    color: var(--color-error);
  }
</style>
