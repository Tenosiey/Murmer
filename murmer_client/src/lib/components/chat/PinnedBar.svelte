<!-- Bar above the message list showing pinned messages for the current channel. -->
<script lang="ts">
  import { pinned } from '$lib/stores/pins';
  import type { PinnedEntry } from '$lib/stores/pins';
  import type { Message } from '$lib/types';
  import { PIN_PREVIEW_LIMIT } from '$lib/chat/constants';

  export let channelId: number;
  export let entries: PinnedEntry[];
  /** Messages of the current channel, used to resolve fresh content for a pin. */
  export let messages: Message[];
  export let onFocusMessage: (messageId: number) => void;

  function resolvePinnedMessage(entry: PinnedEntry): Message | undefined {
    return messages.find((message) => message.id === entry.id);
  }

  function formatPinnedPreview(entry: PinnedEntry): string {
    const message = resolvePinnedMessage(entry);
    const base = message?.text ?? entry.text ?? '';
    const trimmed = base.trim();
    if (trimmed.length > 0) {
      return trimmed.length > PIN_PREVIEW_LIMIT ? `${trimmed.slice(0, PIN_PREVIEW_LIMIT)}…` : trimmed;
    }
    if (message?.image || entry.image) {
      return 'Image attachment';
    }
    return 'Message';
  }

  function pinnedAuthor(entry: PinnedEntry): string {
    const message = resolvePinnedMessage(entry);
    return message?.user ?? entry.user ?? 'Unknown';
  }

  function pinnedTimestamp(entry: PinnedEntry): string {
    const source = resolvePinnedMessage(entry)?.timestamp ?? entry.timestamp ?? entry.pinnedAt;
    if (!source) return '';
    const parsed = Date.parse(source);
    if (Number.isNaN(parsed)) return '';
    return new Date(parsed).toLocaleString();
  }
</script>

{#if entries.length > 0}
  <div class="pinned-bar" role="region" aria-label="Pinned messages">
    <div class="pinned-header">
      <span class="pinned-title">Pinned</span>
      <span class="pinned-count">{entries.length}</span>
    </div>
    <ul class="pinned-list">
      {#each entries as entry (entry.id)}
        <li class="pinned-item">
          <button class="pinned-preview" on:click={() => onFocusMessage(entry.id)}>
            <span class="pinned-author">{pinnedAuthor(entry)}</span>
            <span class="pinned-text">{formatPinnedPreview(entry)}</span>
            {#if pinnedTimestamp(entry)}
              <span class="pinned-timestamp">{pinnedTimestamp(entry)}</span>
            {/if}
          </button>
          <button
            class="pinned-remove"
            on:click={() => pinned.unpin(channelId, entry.id)}
            aria-label="Unpin message"
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
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    border-radius: var(--radius-lg);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 90%, transparent);
  }

  .pinned-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .pinned-title {
    font-size: 0.95rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .pinned-count {
    font-size: 0.75rem;
    padding: 0.1rem 0.6rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-primary) 16%, transparent);
    color: var(--color-on-surface);
  }

  .pinned-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .pinned-item {
    display: flex;
    gap: 0.5rem;
    align-items: stretch;
  }

  .pinned-preview {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    text-align: left;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 16%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    padding: 0.6rem 0.75rem;
    color: var(--color-on-surface);
    cursor: pointer;
    transition: background var(--transition), border-color var(--transition), transform var(--transition);
  }

  .pinned-preview:hover {
    background: color-mix(in srgb, var(--color-primary) 14%, transparent);
    border-color: color-mix(in srgb, var(--color-primary) 28%, transparent);
    transform: translateY(-1px);
  }

  .pinned-author {
    font-weight: 600;
    font-size: 0.9rem;
  }

  .pinned-text {
    font-size: 0.88rem;
    color: color-mix(in srgb, var(--color-on-surface) 88%, transparent);
    word-break: break-word;
  }

  .pinned-timestamp {
    font-size: 0.75rem;
    color: var(--color-muted);
  }

  .pinned-remove {
    border: none;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
    color: var(--color-on-surface);
    width: 30px;
    height: 30px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background var(--transition), transform var(--transition);
  }

  .pinned-remove:hover {
    background: color-mix(in srgb, #ef4444 22%, transparent);
    transform: translateY(-1px);
  }

  .pinned-remove:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--color-primary) 40%, transparent);
    outline-offset: 2px;
  }
</style>
