<script lang="ts">

  import { tick } from 'svelte';
  import type { Message } from '$lib/types';
  import { searchResultPreview, formatSearchTimestamp, ephemeralInfo } from '$lib/chat/helpers';

  interface Props {
    open: boolean;
    onClose: () => void;
    onSearch: (query: string) => Promise<Message[]>;
    onFocusResult: (msg: Message) => void;
    now: number;
  }

  let {
    open,
    onClose,
    onSearch,
    onFocusResult,
    now
  }: Props = $props();

  let query = $state('');
  let results: Message[] = $state([]);
  let loading = $state(false);
  let error: string | null = $state(null);
  let performed = $state(false);
  let inputEl: HTMLInputElement | null = $state(null);

  export function openWith(initialQuery = '') {
    query = initialQuery;
    results = [];
    loading = false;
    error = null;
    performed = false;
    tick().then(() => {
      if (inputEl) {
        inputEl.focus();
        if (initialQuery) inputEl.select();
      }
    });
  }

  async function performSearch() {
    const trimmed = query.trim();
    if (!trimmed) {
      error = 'Enter a search query.';
      results = [];
      performed = false;
      return;
    }
    loading = true;
    error = null;
    try {
      results = await onSearch(trimmed);
      performed = true;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Search failed.';
      results = [];
      performed = true;
    } finally {
      loading = false;
    }
  }

  function focusResult(result: Message) {
    if (typeof result.id !== 'number') return;
    onClose();
    onFocusResult(result);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
    }
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onClose();
    }
  }

  export function triggerSearch() {
    void performSearch();
  }
</script>

{#if open}
  <div
    class="search-overlay"
    role="button"
    tabindex="0"
    aria-label="Close search results"
    onclick={onClose}
    onkeydown={handleOverlayKeydown}
  >
    <div
      class="search-panel"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleKeydown}
    >
      <form class="search-form" onsubmit={(event) => { event.preventDefault(); performSearch(); }}>
        <input
          type="search"
          placeholder="Search messages"
          aria-label="Search messages"
          bind:value={query}
          bind:this={inputEl}
        />
        <button type="submit" class="btn btn-primary search-submit" disabled={loading}>Search</button>
        <button type="button" class="btn search-close" onclick={onClose}>Close</button>
      </form>
      {#if error}
        <p class="search-error">{error}</p>
      {/if}
      {#if loading}
        <p class="search-status">Searching…</p>
      {:else if results.length > 0}
        <ul class="search-results">
          {#each results as result (result.id ?? `${result.timestamp ?? ''}-${result.user ?? ''}`)}
            <li>
              <button type="button" class="search-result" onclick={() => focusResult(result)}>
                <span class="search-result-text">{searchResultPreview(result)}</span>
                <span class="search-result-meta">
                  <span class="search-result-user">{result.user ?? 'Unknown'}</span>
                  <span class="search-result-time">{formatSearchTimestamp(result)}</span>
                </span>
                {#if result.ephemeral}
                  {@const info = ephemeralInfo(result, now)}
                  {#if info}
                    <span class="search-result-ephemeral">{info.label}</span>
                  {/if}
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {:else if performed}
        <p class="search-status">No matches found.</p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .search-overlay {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: var(--space-8) var(--space-5) var(--space-5);
    z-index: var(--z-overlay);
  }

  .search-panel {
    width: min(640px, 92vw);
    max-height: min(70vh, 640px);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    background: var(--color-surface-elevated);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-lg);
    padding: var(--space-4);
  }

  .search-form {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .search-form input {
    flex: 1;
    min-width: 12rem;
    border-radius: var(--radius-md);
  }

  .search-error {
    margin: 0;
    color: var(--color-error);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .search-status {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
    text-align: center;
    padding: var(--space-4) 0;
  }

  .search-results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    overflow-y: auto;
  }

  .search-result {
    width: 100%;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    color: var(--color-on-surface);
    padding: var(--space-2) var(--space-3);
  }

  .search-result:hover {
    background: var(--color-surface-raised);
  }

  .search-result-text {
    font-weight: 500;
    font-size: var(--text-md);
    overflow-wrap: anywhere;
  }

  .search-result-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .search-result-time {
    font-family: var(--font-mono);
  }

  .search-result-ephemeral {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-warning);
  }
</style>
