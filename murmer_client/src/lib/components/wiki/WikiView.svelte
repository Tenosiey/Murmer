<!--
  Full-width channel wiki view: a page-list sidebar plus a viewer/editor
  pane. Replaces the message area while open. Page management (create,
  rename, delete) is gated by `canEdit`; the server enforces permissions
  regardless.
-->
<script lang="ts">
  import { wiki, type WikiPage } from '$lib/stores/wiki';
  import { wikilinks, slugify, type WikiNavigation } from '$lib/wiki/links';
  import WikiEditor from './WikiEditor.svelte';
  import ContextMenu from '$lib/components/ContextMenu.svelte';
  import { dialogs } from '$lib/stores/dialogs';
  import { renderMarkdown } from '$lib/markdown';
  import { emojifyHtml } from '$lib/emoji';
  import { customEmojis } from '$lib/stores/customEmojis';
  import { selectedServer } from '$lib/stores/servers';
  import { httpBaseFromWs } from '$lib/server-url';
  import type { ContextMenuItem } from '$lib/types';

  
  
  interface Props {
    channelId: number;
    channelName: string;
    canEdit?: boolean;
    /** Page to open initially (e.g. from a cross-channel link). */
    initialSlug?: string | null;
    /** A `[[other-channel/page]]` link was clicked; the parent switches channels. */
    onCrossChannel?: (channel: string, slug: string) => void;
  }

  let {
    channelId,
    channelName,
    canEdit = false,
    initialSlug = null,
    onCrossChannel = () => {}
  }: Props = $props();

  /* Deliberately captures the slug the view opened with; later navigation
     is driven by selectPage, not by the prop. */
  // svelte-ignore state_referenced_locally
  let selectedSlug: string | null = $state(initialSlug);
  let mode: 'view' | 'edit' = $state('view');
  let currentPage: WikiPage | null = $state(null);
  let loading = $state(false);
  let editorDirty = $state(false);

  let menuOpen = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);
  let menuSlug = $state('');



  let lastFetchKey = '';

  async function loadSelected() {
    if (!selectedSlug) {
      currentPage = null;
      return;
    }
    const slug = selectedSlug;
    loading = true;
    try {
      const page = await wiki.getPage(channelId, slug);
      if (slug === selectedSlug) {
        currentPage = page;
      }
    } catch {
      // Timed out (e.g. reconnecting) — the next index change retries.
    } finally {
      if (slug === selectedSlug) {
        loading = false;
      }
    }
  }

  /** True when it is safe to leave the editor (nothing dirty or confirmed). */
  async function confirmDiscard(): Promise<boolean> {
    if (mode !== 'edit' || !editorDirty) return true;
    return dialogs.confirm({
      title: 'Discard changes?',
      message: 'Your unsaved wiki edits will be lost.',
      confirmLabel: 'Discard',
      danger: true
    });
  }

  async function selectPage(slug: string) {
    if (slug === selectedSlug && mode === 'view') return;
    if (!(await confirmDiscard())) return;
    mode = 'view';
    selectedSlug = slug;
  }

  function titleFromSlug(slug: string): string {
    return slug
      .split('-')
      .filter(Boolean)
      .map((word) => word[0].toUpperCase() + word.slice(1))
      .join(' ');
  }

  async function createPagePrompt(prefillSlug?: string) {
    const title = await dialogs.prompt({
      title: 'New wiki page',
      label: 'Page title',
      initial: prefillSlug ? titleFromSlug(prefillSlug) : '',
      maxLength: 100
    });
    if (!title) return;
    const slug = prefillSlug ?? slugify(title);
    if (!slug) {
      await dialogs.alert({ title: 'Wiki', message: 'That title does not produce a usable page name.' });
      return;
    }
    if (pages.some((p) => p.slug === slug)) {
      // Already exists — just open it.
      await selectPage(slug);
      return;
    }
    wiki.createPage(channelId, slug, title.trim());
    // The page appears via the index broadcast; select it now so the view
    // shows it as soon as it lands.
    mode = 'view';
    selectedSlug = slug;
  }

  async function handleNavigate(nav: WikiNavigation) {
    if (nav.channel && nav.channel !== channelName) {
      onCrossChannel(nav.channel, nav.slug);
      return;
    }
    if (!(await confirmDiscard())) return;
    mode = 'view';
    if (!pages.some((p) => p.slug === nav.slug)) {
      if (!canEdit) {
        await dialogs.alert({ title: 'Wiki', message: 'This page does not exist yet.' });
        return;
      }
      const create = await dialogs.confirm({
        title: 'Create wiki page?',
        message: `"${nav.slug}" does not exist in this channel yet.`,
        confirmLabel: 'Create'
      });
      if (!create) return;
      wiki.createPage(channelId, nav.slug, titleFromSlug(nav.slug));
    }
    selectedSlug = nav.slug;
  }

  function openPageMenu(event: MouseEvent, slug: string) {
    if (!canEdit) return;
    event.preventDefault();
    menuSlug = slug;
    menuX = event.clientX;
    menuY = event.clientY;
    menuOpen = true;
  }

  async function renamePrompt(slug: string) {
    const input = await dialogs.prompt({
      title: 'Rename wiki page',
      label: 'Page name (slug)',
      initial: slug,
      message: 'Links to the old name will show as missing pages.',
      maxLength: 64
    });
    if (!input) return;
    const newSlug = slugify(input);
    if (!newSlug || newSlug === slug) return;
    wiki.renamePage(channelId, slug, newSlug);
    if (selectedSlug === slug) {
      selectedSlug = newSlug;
    }
  }

  async function deletePrompt(slug: string) {
    const meta = pages.find((p) => p.slug === slug);
    const confirmed = await dialogs.confirm({
      title: 'Delete wiki page?',
      message: `"${meta?.title ?? slug}" and its history will be removed for everyone.`,
      confirmLabel: 'Delete',
      danger: true
    });
    if (!confirmed) return;
    wiki.deletePage(channelId, slug);
    if (selectedSlug === slug) {
      selectedSlug = null;
      currentPage = null;
    }
  }

  async function startEditing() {
    if (!currentPage) return;
    mode = 'edit';
  }


  function formatTimestamp(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }
  let pages = $derived($wiki[channelId] ?? []);
  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');
  // Default to the first page (the list arrives title-sorted).
  $effect(() => {
    if (selectedSlug === null && pages.length > 0) {
      selectedSlug = pages[0].slug;
    }
  });
  // Reload the page body when the selection changes or a remote save bumps
  // the selected page's revision in the index.
  let selectedMeta = $derived(selectedSlug ? (pages.find((p) => p.slug === selectedSlug) ?? null) : null);
  let fetchKey = $derived(`${channelId}:${selectedSlug ?? ''}:${selectedMeta?.revision ?? 0}`);
  $effect(() => {
    if (fetchKey !== lastFetchKey) {
      lastFetchKey = fetchKey;
      void loadSelected();
    }
  });
  let pageMenuItems = $derived([
    { label: 'Rename', action: () => void renamePrompt(menuSlug) },
    { label: 'Delete', danger: true, action: () => void deletePrompt(menuSlug) }
  ] satisfies ContextMenuItem[]);
</script>

<div class="wiki">
  <aside class="sidebar">
    <div class="sidebar-header">
      <h2>Wiki</h2>
      {#if canEdit}
        <button class="icon-btn" onclick={() => createPagePrompt()} title="New wiki page">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M12 5v14" />
            <path d="M5 12h14" />
          </svg>
          <span class="sr-only">New wiki page</span>
        </button>
      {/if}
    </div>
    {#if pages.length > 0}
      <ul class="page-list">
        {#each pages as page (page.slug)}
          <li>
            <button
              class="page-item"
              class:active={page.slug === selectedSlug}
              onclick={() => selectPage(page.slug)}
              oncontextmenu={(e) => openPageMenu(e, page.slug)}
              title={page.slug}
            >
              {page.title}
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="sidebar-empty">No pages yet</p>
    {/if}
  </aside>

  <section class="main">
    {#if mode === 'edit' && currentPage}
      {#key `${channelId}:${selectedSlug}`}
        <WikiEditor
          {channelId}
          {channelName}
          page={currentPage}
          bind:dirty={editorDirty}
          onSaved={() => {
            mode = 'view';
            editorDirty = false;
          }}
          onCancel={async () => {
            if (await confirmDiscard()) {
              mode = 'view';
            }
          }}
        />
      {/key}
    {:else if currentPage}
      <div class="page-header">
        <div class="page-title">
          <h3>{currentPage.title}</h3>
          <span class="page-meta">
            updated by {currentPage.updatedBy || currentPage.author || 'unknown'} · rev
            {currentPage.revision} · {formatTimestamp(currentPage.updatedAt)}
          </span>
        </div>
        {#if canEdit}
          <button class="btn btn-ghost" onclick={startEditing}>Edit</button>
        {/if}
      </div>
      <div
        class="page-body wiki-body"
        use:wikilinks={{ channelName, onNavigate: (nav) => void handleNavigate(nav) }}
      >
        {@html emojifyHtml(renderMarkdown(currentPage.body), $customEmojis, httpBase)}
      </div>
    {:else if loading}
      <div class="placeholder">Loading page…</div>
    {:else if pages.length === 0}
      <div class="placeholder">
        <p>This channel has no wiki pages yet.</p>
        {#if canEdit}
          <button class="btn btn-primary" onclick={() => createPagePrompt()}>
            Create the first page
          </button>
        {:else}
          <p class="placeholder-hint">Moderators can create pages here.</p>
        {/if}
      </div>
    {:else}
      <div class="placeholder">
        <p>This page does not exist{selectedSlug ? ` (${selectedSlug})` : ''}.</p>
        {#if canEdit && selectedSlug}
          <button class="btn btn-primary" onclick={() => createPagePrompt(selectedSlug ?? undefined)}>
            Create it
          </button>
        {/if}
      </div>
    {/if}
  </section>

  <ContextMenu bind:open={menuOpen} x={menuX} y={menuY} items={pageMenuItems} />
</div>

<style>
  .wiki {
    display: flex;
    flex: 1;
    min-height: 0;
    background: var(--color-bg);
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    width: 14rem;
    flex-shrink: 0;
    border-right: 1px solid var(--color-surface-outline);
    background: var(--color-surface);
    overflow-y: auto;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-3) var(--space-2);
  }

  .sidebar-header h2 {
    font-size: var(--text-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-muted);
    margin: 0;
  }

  .page-list {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2) var(--space-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .page-item {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--color-on-surface-variant);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    font-size: var(--text-md);
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .page-item:hover,
  .page-item:focus-visible {
    background: var(--color-surface-raised);
    outline: none;
  }

  .page-item.active {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
    font-weight: 600;
  }

  .sidebar-empty {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-sm);
    color: var(--color-muted);
    font-style: italic;
  }

  .main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    padding: var(--space-4);
    gap: var(--space-3);
  }

  .page-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .page-title h3 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
  }

  .page-meta {
    display: block;
    margin-top: var(--space-1);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .page-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: var(--space-2);
  }

  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    flex: 1;
    color: var(--color-muted);
    text-align: center;
  }

  .placeholder p {
    margin: 0;
  }

  .placeholder-hint {
    font-size: var(--text-sm);
    font-style: italic;
  }
</style>
