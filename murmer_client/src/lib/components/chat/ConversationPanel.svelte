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

  export let title: string;
  export let kind: 'thread' | 'dm' = 'thread';
  export let messages: Message[] = [];
  export let emptyText = 'No messages yet.';
  export let placeholder = 'Reply…';
  export let onSend: (text: string) => void;
  export let onClose: () => void;
  export let emphasize: (msg: Message) => boolean = () => false;

  let draft = '';

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
    <button type="button" class="icon-btn" on:click={onClose} aria-label="Close panel">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
    </button>
  </header>

  <div class="messages">
    {#each messages as msg (msg.id)}
      <div class="entry" class:emphasized={emphasize(msg)}>
        <div class="entry-meta">
          <span class="username">{kind === 'dm' ? msg.from : msg.user}</span>
          <span class="timestamp">{msg.time}</span>
        </div>
        <div class="entry-text">
          {#if msg.text}
            {@html renderMarkdown(msg.text)}
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

  <form class="reply" on:submit|preventDefault={submit}>
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
