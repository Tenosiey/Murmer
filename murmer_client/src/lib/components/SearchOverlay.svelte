<script lang="ts">
  import { tick } from 'svelte';
  import type { Message } from '$lib/types';
  import { searchResultPreview, formatSearchTimestamp, ephemeralInfo } from '$lib/chat/helpers';

  export let open: boolean;
  export let onClose: () => void;
  export let onSearch: (query: string) => Promise<Message[]>;
  export let onFocusResult: (msg: Message) => void;
  export let now: number;

  let query = '';
  let results: Message[] = [];
  let loading = false;
  let error: string | null = null;
  let performed = false;
  let inputEl: HTMLInputElement | null = null;

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
    on:click={onClose}
    on:keydown={handleOverlayKeydown}
  >
    <div
      class="search-panel"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      on:click|stopPropagation
      on:keydown={handleKeydown}
    >
      <form class="search-form" on:submit|preventDefault={performSearch}>
        <input
          type="search"
          placeholder="Search messages"
          aria-label="Search messages"
          bind:value={query}
          bind:this={inputEl}
        />
        <button type="submit" class="search-submit" disabled={loading}>Search</button>
        <button type="button" class="search-close" on:click={onClose}>Close</button>
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
              <button type="button" class="search-result" on:click={() => focusResult(result)}>
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
    background: color-mix(in srgb, var(--color-overlay) 85%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: clamp(1.5rem, 4vw, 3rem);
    z-index: 60;
  }

  .search-panel {
    width: min(720px, 92vw);
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    background: color-mix(in srgb, var(--color-surface-elevated) 96%, transparent);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-lg);
    padding: clamp(1rem, 3vw, 1.75rem);
  }

  .search-form {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .search-form input {
    flex: 1;
    min-width: 12rem;
    padding: 0.65rem 0.85rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-surface-raised) 92%, transparent);
    color: var(--color-on-surface);
  }

  .search-form input:focus {
    outline: 2px solid color-mix(in srgb, var(--color-primary) 35%, transparent);
    outline-offset: 2px;
  }

  .search-form button {
    padding: 0.6rem 0.95rem;
    border-radius: var(--radius-md);
    font-weight: 600;
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-primary) 16%, transparent);
    color: var(--color-on-surface);
    transition: background var(--transition), transform var(--transition), opacity var(--transition);
  }

  .search-form button:not([disabled]):hover {
    transform: translateY(-1px);
  }

  .search-form button[disabled] {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .search-form .search-close {
    background: color-mix(in srgb, var(--color-muted) 16%, transparent);
    border-color: color-mix(in srgb, var(--color-muted) 24%, transparent);
  }

  .search-error {
    color: color-mix(in srgb, var(--color-error) 85%, var(--color-on-surface) 15%);
    font-weight: 600;
  }

  .search-status {
    color: var(--color-muted);
    font-size: 0.9rem;
  }

  .search-results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    max-height: 320px;
    overflow-y: auto;
  }

  .search-result {
    width: 100%;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 16%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 92%, transparent);
    color: var(--color-on-surface);
    padding: 0.75rem 0.9rem;
    transition: border-color var(--transition), transform var(--transition);
  }

  .search-result:hover {
    transform: translateY(-1px);
    border-color: color-mix(in srgb, var(--color-primary) 32%, transparent);
  }

  .search-result-text {
    font-weight: 600;
  }

  .search-result-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
    font-size: 0.8rem;
    color: var(--color-muted);
  }

  .search-result-ephemeral {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: color-mix(in srgb, var(--color-warning) 75%, var(--color-on-surface) 25%);
  }
</style>
