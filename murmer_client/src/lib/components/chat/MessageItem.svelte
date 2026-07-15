<!--
  A single chat message row. Group heads render an avatar, username, role and
  timestamp; continuation messages (same author within the grouping window)
  render compactly and reveal their timestamp in the gutter on hover.
  A floating action toolbar (react/reply/edit/pin/delete) appears on hover or
  keyboard focus.
-->
<script lang="ts">
  import type { Message } from '$lib/types';
  import { roles } from '$lib/stores/roles';
  import { session } from '$lib/stores/session';
  import { renderMarkdown } from '$lib/markdown';
  import { ephemeralInfo, formatFileSize, reactionEntries } from '$lib/chat/helpers';
  import { giphyGifUrl } from '$lib/link-preview';
  import LinkPreview from '$lib/components/LinkPreview.svelte';
  import UserAvatar from '$lib/components/UserAvatar.svelte';

  export let message: Message;
  export let links: string[] = [];
  export let continuation = false;
  export let now: number;
  export let highlighted = false;
  export let pinned = false;
  export let replyCount = 0;
  export let canEdit = false;
  export let canDelete = false;
  export let canPin = false;

  export let onFocusMessage: (id: number) => void;
  export let onReply: (msg: Message) => void;
  export let onEdit: (msg: Message) => void;
  export let onTogglePin: (msg: Message) => void;
  export let onDelete: (msg: Message) => void;
  export let onOpenEmojiPicker: (id: number, event: MouseEvent) => void;
  export let onToggleReaction: (id: number, emoji: string, users: string[]) => void;
  export let onOpenThread: (id: number) => void;

  $: messageId = typeof message.id === 'number' ? message.id : null;
  $: roleInfo = message.user ? $roles[message.user] : undefined;
  $: reactions = reactionEntries(message);
  $: eInfo = message.ephemeral ? ephemeralInfo(message, now) : null;
  $: hasActions = messageId !== null && (canPin || canDelete);
  /* A message that is nothing but a Giphy link renders as the GIF alone;
     the raw URL text would just duplicate it. */
  $: textIsOnlyGif =
    links.length === 1 &&
    giphyGifUrl(links[0]) !== null &&
    typeof message.text === 'string' &&
    /^https?:\/\/\S+$/.test(message.text.trim());
</script>

<div
  class="message"
  class:continuation
  data-message-id={messageId ?? undefined}
  class:highlighted
>
  <div class="gutter">
    {#if !continuation}
      <UserAvatar name={message.user ?? '?'} />
    {:else}
      <span class="gutter-time" aria-hidden="true">{message.time}</span>
    {/if}
  </div>

  <div class="body">
    {#if message.replyTo}
      {@const reply = message.replyTo}
      <button
        type="button"
        class="reply-quote"
        on:click={() => onFocusMessage(reply.id)}
        title={`Jump to ${reply.user}'s message`}
      >
        <span class="reply-quote-arrow" aria-hidden="true">↪</span>
        <span class="reply-quote-user">{reply.user}</span>
        <span class="reply-quote-text">{reply.text || 'Original message'}</span>
      </button>
    {/if}

    {#if !continuation}
      <div class="meta">
        <span class="username">{message.user}</span>
        {#if message.bot}
          <span class="bot-badge">BOT</span>
        {/if}
        {#if roleInfo}
          <span class="role" style={roleInfo.color ? `color: ${roleInfo.color}` : ''}>
            {roleInfo.role}
          </span>
        {/if}
        <span class="timestamp">{message.time}</span>
      </div>
    {/if}

    <span class="content">
      {#if message.text && !textIsOnlyGif}
        {@html renderMarkdown(message.text)}
      {/if}
      {#if message.edited}
        <span
          class="edited-badge"
          title={message.editedAt ? `Edited ${new Date(message.editedAt).toLocaleString()}` : 'Edited'}
        >
          (edited)
        </span>
      {/if}
      {#if links.length > 0}
        <div class="link-previews">
          {#each links as link (link)}
            <LinkPreview url={link} />
          {/each}
        </div>
      {/if}
      {#if message.image}
        <img src={message.image as string} alt="" loading="lazy" />
      {/if}
      {#if message.attachment}
        <a
          class="attachment-card"
          href={message.attachment.url}
          download={message.attachment.name}
          target="_blank"
          rel="noopener noreferrer"
        >
          <span class="attachment-icon" aria-hidden="true">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>
          </span>
          <span class="attachment-details">
            <span class="attachment-name">{message.attachment.name}</span>
            {#if message.attachment.size > 0}
              <span class="attachment-size">{formatFileSize(message.attachment.size)}</span>
            {/if}
          </span>
        </a>
      {/if}
      {#if eInfo}
        <span class="ephemeral-badge" title={eInfo.absolute ?? undefined}>{eInfo.label}</span>
      {/if}
    </span>

    {#if messageId !== null && reactions.length > 0}
      <div class="reactions">
        {#each reactions as reaction (reaction.emoji)}
          <button
            class="reaction-chip"
            class:active={reaction.users.includes($session.user ?? '')}
            on:click={() => onToggleReaction(messageId, reaction.emoji, reaction.users)}
            title={reaction.users.join(', ')}
          >
            <span class="emoji">{reaction.emoji}</span>
            <span class="count">{reaction.users.length}</span>
          </button>
        {/each}
        <button
          class="reaction-chip add"
          on:click={(e) => onOpenEmojiPicker(messageId, e)}
          title="Add reaction"
        >
          +
        </button>
      </div>
    {/if}

    {#if messageId !== null && replyCount > 0}
      <button
        type="button"
        class="thread-indicator"
        on:click={() => onOpenThread(messageId)}
        title="Open thread"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
        {replyCount} {replyCount === 1 ? 'reply' : 'replies'}
      </button>
    {/if}
  </div>

  {#if hasActions && messageId !== null}
    <div class="message-actions">
      <button
        type="button"
        class="message-action"
        on:click={(e) => onOpenEmojiPicker(messageId, e)}
        title="Add reaction"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>
        <span class="sr-only">Add reaction</span>
      </button>
      <button type="button" class="message-action" on:click={() => onReply(message)} title="Reply">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="9 17 4 12 9 7"/><path d="M20 18v-2a4 4 0 0 0-4-4H4"/></svg>
        <span class="sr-only">Reply</span>
      </button>
      {#if canEdit}
        <button type="button" class="message-action" on:click={() => onEdit(message)} title="Edit message">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/></svg>
          <span class="sr-only">Edit message</span>
        </button>
      {/if}
      {#if canPin}
        <button
          type="button"
          class="message-action"
          class:active={pinned}
          on:click={() => onTogglePin(message)}
          title={pinned ? 'Unpin message' : 'Pin message'}
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>
          <span class="sr-only">{pinned ? 'Unpin message' : 'Pin message'}</span>
        </button>
      {/if}
      {#if canDelete}
        <button
          type="button"
          class="message-action danger"
          on:click={() => onDelete(message)}
          title="Delete message"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 11v6"/><path d="M14 11v6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
          <span class="sr-only">Delete message</span>
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* Two-column grid: fixed gutter (avatar / hover timestamp), then content.
     Group heads get breathing room; continuations stay tight. */
  .message {
    position: relative;
    display: grid;
    grid-template-columns: var(--space-6) minmax(0, 1fr);
    column-gap: var(--space-3);
    padding: var(--space-1) var(--space-4);
    margin-top: var(--space-3);
    border-left: 2px solid transparent;
    /* Isolate layout/style recalculation per message so long histories stay cheap. */
    contain: layout style;
  }

  .message.continuation {
    margin-top: 0;
  }

  .message:hover {
    background: color-mix(in srgb, var(--color-surface-raised) 45%, transparent);
  }

  .message.highlighted {
    background: color-mix(in srgb, var(--color-primary) 10%, transparent);
    border-left-color: var(--color-primary);
  }

  .gutter {
    display: flex;
    justify-content: center;
    padding-top: 2px;
  }

  .gutter-time {
    align-self: start;
    font-size: 0.625rem;
    font-family: var(--font-mono);
    color: var(--color-muted);
    line-height: 1.55rem;
    opacity: 0;
    white-space: nowrap;
  }

  .message:hover .gutter-time,
  .message:focus-within .gutter-time {
    opacity: 1;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  .meta {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }

  .username {
    font-weight: 600;
    font-size: var(--text-md);
    color: var(--color-on-surface);
  }

  .timestamp {
    font-size: var(--text-xs);
    color: var(--color-muted);
    font-family: var(--font-mono);
  }

  .role {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-muted);
  }

  .bot-badge {
    display: inline-flex;
    align-items: center;
    align-self: center;
    padding: 0 var(--space-1);
    border-radius: var(--radius-xs);
    font-size: 0.625rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    line-height: 1rem;
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .reply-quote {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    max-width: 100%;
    padding: var(--space-1) var(--space-2);
    border: none;
    border-left: 2px solid var(--color-outline-strong);
    border-radius: 0 var(--radius-xs) var(--radius-xs) 0;
    background: color-mix(in srgb, var(--color-surface-raised) 55%, transparent);
    color: var(--color-muted);
    font-size: var(--text-sm);
    cursor: pointer;
    text-align: left;
  }

  .reply-quote:hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .reply-quote-arrow {
    flex-shrink: 0;
    opacity: 0.7;
  }

  .reply-quote-user {
    flex-shrink: 0;
    font-weight: 600;
    color: var(--color-on-surface-variant);
  }

  .reply-quote-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .content {
    display: block;
    color: var(--color-on-surface-variant);
    font-size: var(--text-md);
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .content :global(p) {
    margin: 0;
  }

  .edited-badge {
    margin-left: var(--space-1);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .ephemeral-badge {
    display: inline-flex;
    align-items: center;
    margin-top: var(--space-2);
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    line-height: 1.25rem;
    background: color-mix(in srgb, var(--color-warning) 15%, transparent);
    color: var(--color-warning);
    width: fit-content;
  }

  .link-previews {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .content :global(code) {
    background: var(--color-surface-raised);
    padding: 0.125rem var(--space-1);
    border-radius: var(--radius-xs);
    font-family: var(--font-mono);
    font-size: 0.85em;
  }

  .content :global(pre) {
    background: var(--color-bg);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    overflow-x: auto;
    border: 1px solid var(--color-surface-outline);
  }

  .content :global(pre code) {
    display: block;
    padding: 0;
    margin: 0;
    background: transparent;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .content :global(.hljs) {
    color: var(--color-on-surface);
  }

  .content :global(.hljs-comment),
  .content :global(.hljs-quote) {
    color: var(--color-muted);
    font-style: italic;
  }

  .content :global(.hljs-keyword),
  .content :global(.hljs-selector-tag),
  .content :global(.hljs-subst) {
    color: color-mix(in srgb, var(--color-primary) 80%, var(--color-on-surface) 20%);
  }

  .content :global(.hljs-string),
  .content :global(.hljs-doctag),
  .content :global(.hljs-regexp) {
    color: color-mix(in srgb, var(--color-success) 75%, var(--color-on-surface) 25%);
  }

  .content :global(.hljs-title),
  .content :global(.hljs-section),
  .content :global(.hljs-function),
  .content :global(.hljs-name) {
    color: color-mix(in srgb, var(--color-primary) 78%, var(--color-on-surface) 22%);
  }

  .content :global(.hljs-number),
  .content :global(.hljs-literal),
  .content :global(.hljs-symbol),
  .content :global(.hljs-bullet) {
    color: color-mix(in srgb, var(--color-warning) 75%, var(--color-on-surface) 25%);
  }

  .content :global(.hljs-attr),
  .content :global(.hljs-attribute),
  .content :global(.hljs-variable),
  .content :global(.hljs-template-variable) {
    color: color-mix(in srgb, var(--color-error) 70%, var(--color-on-surface) 30%);
  }

  .content :global(.hljs-meta),
  .content :global(.hljs-meta .hljs-string) {
    color: color-mix(in srgb, var(--color-primary) 65%, var(--color-on-surface) 35%);
  }

  .content img {
    max-width: min(420px, 100%);
    border-radius: var(--radius-md);
    margin-top: var(--space-2);
    border: 1px solid var(--color-surface-outline);
  }

  .attachment-card {
    display: inline-flex;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
    color: var(--color-on-surface);
    text-decoration: none;
    max-width: min(420px, 100%);
  }

  .attachment-card:hover {
    border-color: var(--color-outline-strong);
    background: var(--color-surface-raised);
    text-decoration: none;
  }

  .attachment-icon {
    display: inline-flex;
    color: var(--color-primary);
    flex-shrink: 0;
  }

  .attachment-details {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .attachment-name {
    font-weight: 500;
    font-size: var(--text-md);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .attachment-size {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }

  .reaction-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    border-radius: var(--radius-pill);
    padding: 0.125rem var(--space-2);
    font-size: var(--text-sm);
    line-height: 1.25rem;
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
    color: var(--color-on-surface-variant);
  }

  .reaction-chip:hover {
    border-color: var(--color-outline-strong);
  }

  .reaction-chip.active {
    background: var(--color-primary-container);
    border-color: color-mix(in srgb, var(--color-primary) 45%, transparent);
    color: var(--color-on-surface);
  }

  .reaction-chip.add {
    color: var(--color-muted);
    font-weight: 600;
  }

  .thread-indicator {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0.125rem var(--space-2);
    border-radius: var(--radius-pill);
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-primary);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
  }

  .thread-indicator:hover {
    background: var(--color-primary-container);
  }

  /* Floating per-message toolbar, shown on hover or keyboard focus. */
  .message-actions {
    position: absolute;
    top: calc(-1 * var(--space-3));
    right: var(--space-4);
    display: inline-flex;
    align-items: center;
    gap: 0;
    padding: var(--space-1);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
    box-shadow: var(--shadow-sm);
    opacity: 0;
    pointer-events: none;
    z-index: 2;
  }

  .message:hover .message-actions,
  .message:focus-within .message-actions {
    opacity: 1;
    pointer-events: auto;
  }

  .message-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    padding: 0;
    border-radius: var(--radius-xs);
    border: none;
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
  }

  .message-action:hover,
  .message-action.active {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .message-action.active {
    color: var(--color-primary);
  }

  .message-action.danger:hover {
    background: color-mix(in srgb, var(--color-error) 14%, transparent);
    color: var(--color-error);
  }
</style>
