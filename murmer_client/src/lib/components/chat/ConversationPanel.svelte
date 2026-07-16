<!--
  Sliding side panel for secondary conversations: message threads and direct
  messages share this layout (header, scrolling message list, reply input).
  The `emphasize` callback marks messages to highlight — the thread root or
  the viewer's own DMs.
-->
<script lang="ts">

  import type { Message } from '$lib/types';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { renderMarkdown } from '$lib/markdown';
  import { emojifyHtml } from '$lib/emoji';
  import { customEmojis } from '$lib/stores/customEmojis';
  import { selectedServer } from '$lib/stores/servers';
  import { httpBaseFromWs } from '$lib/server-url';
  import { formatFullTimestamp, formatShortTime } from '$lib/chat/helpers';

  interface Props {
    title: string;
    kind?: 'thread' | 'dm';
    messages?: Message[];
    emptyText?: string;
    placeholder?: string;
    onSend: (text: string) => void;
    onClose: () => void;
    emphasize?: (msg: Message) => boolean;
    /** DMs: the peer's identity key changed and is not yet trusted. */
    keyWarning?: boolean;
    /** DMs: accept the peer's changed key after out-of-band verification. */
    onTrustKey?: () => void;
    /** DMs: show the conversation's key fingerprint for verification. */
    onVerify?: () => void;
  }

  let {
    title,
    kind = 'thread',
    messages = [],
    emptyText = 'No messages yet.',
    placeholder = 'Reply…',
    onSend,
    onClose,
    emphasize = () => false,
    keyWarning = false,
    onTrustKey,
    onVerify
  }: Props = $props();

  let draft = $state('');

  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');

  function submit() {
    const trimmed = draft.trim();
    if (trimmed === '') return;
    onSend(trimmed);
    draft = '';
  }
</script>

<aside
  class="panel"
  aria-label={kind === 'dm' ? 'Direct messages' : 'Thread'}
  transition:fly={{ x: 48, duration: 160, easing: cubicOut }}
>
  <header>
    <span class="title">
      {#if kind === 'dm'}
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/></svg>
      {:else}
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
      {/if}
      {title}
    </span>
    <span class="header-actions">
      {#if kind === 'dm'}
        <button
          type="button"
          class="icon-btn lock"
          onclick={onVerify}
          title="End-to-end encrypted — click to verify"
          aria-label="Verify encryption keys"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
        </button>
      {/if}
      <button type="button" class="icon-btn" onclick={onClose} aria-label="Close panel">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
      </button>
    </span>
  </header>

  {#if keyWarning}
    <div class="key-warning" role="alert">
      <p>
        {title}'s security key has changed. This happens after a key reset — but it can also mean
        someone else is answering under this name. Verify the new fingerprint out-of-band before
        trusting it.
      </p>
      <div class="key-warning-actions">
        <button type="button" class="btn btn-ghost" onclick={onVerify}>Show fingerprint</button>
        <button type="button" class="btn btn-danger" onclick={onTrustKey}>Trust new key</button>
      </div>
    </div>
  {/if}

  <div class="messages">
    {#each messages as msg (msg.id)}
      <div class="entry" class:emphasized={emphasize(msg)}>
        <div class="entry-meta">
          <span class="username">{kind === 'dm' ? msg.from : msg.user}</span>
          <span class="timestamp" title={formatFullTimestamp(msg)}>{formatShortTime(msg)}</span>
        </div>
        <div class="entry-text">
          {#if msg.decryptFailed}
            <span class="undecryptable">This message could not be decrypted.</span>
          {:else if msg.text}
            {@html emojifyHtml(renderMarkdown(msg.text), $customEmojis, httpBase)}
          {:else if msg.image}
            <img src={msg.image as string} alt="" loading="lazy" />
          {:else if msg.attachment}
            <a href={msg.attachment.url} target="_blank" rel="noopener noreferrer">
              {msg.attachment.name}
            </a>
          {/if}
        </div>
      </div>
    {:else}
      <p class="empty">{emptyText}</p>
    {/each}
  </div>

  <form class="reply" onsubmit={(event) => { event.preventDefault(); submit(); }}>
    <input type="text" bind:value={draft} {placeholder} aria-label={placeholder} />
  </form>
</aside>

<style>
  .panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 25;
    width: min(360px, 90%);
    display: flex;
    flex-direction: column;
    background: var(--color-surface-elevated);
    border-left: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-md);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3) var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--color-surface-outline);
    font-weight: 600;
    font-size: var(--text-md);
  }

  .title {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title svg {
    color: var(--color-primary);
    flex-shrink: 0;
  }

  .header-actions {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    flex-shrink: 0;
  }

  .header-actions .lock {
    color: var(--color-success);
  }

  .key-warning {
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid color-mix(in srgb, var(--color-warning) 45%, transparent);
    background: color-mix(in srgb, var(--color-warning) 12%, transparent);
  }

  .key-warning p {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    line-height: 1.5;
    color: var(--color-on-surface);
  }

  .key-warning-actions {
    display: flex;
    gap: var(--space-2);
    justify-content: flex-end;
  }

  .undecryptable {
    font-style: italic;
    color: var(--color-muted);
  }

  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
  }

  .entry {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
  }

  .entry.emphasized {
    background: var(--color-primary-container);
  }

  .entry-meta {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }

  .entry-meta .username {
    font-weight: 600;
    color: var(--color-on-surface);
    font-size: var(--text-sm);
  }

  .entry-meta .timestamp {
    font-size: var(--text-xs);
    color: var(--color-muted);
    font-family: var(--font-mono);
  }

  .entry-text {
    color: var(--color-on-surface-variant);
    font-size: var(--text-sm);
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .entry-text :global(p) {
    margin: 0;
  }

  .entry-text :global(img.inline-emoji) {
    width: 1.25rem;
    height: 1.25rem;
    object-fit: contain;
    vertical-align: -0.3em;
    margin-top: 0;
  }

  .entry-text img {
    max-width: 100%;
    border-radius: var(--radius-sm);
    margin-top: var(--space-1);
  }

  .empty {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
    text-align: center;
    padding: var(--space-4);
  }

  .reply {
    padding: var(--space-3);
    border-top: 1px solid var(--color-surface-outline);
  }

  .reply input {
    width: 100%;
    border-radius: var(--radius-md);
  }
</style>
