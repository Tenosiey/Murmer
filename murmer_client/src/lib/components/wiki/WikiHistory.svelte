<!--
  Revision history for one wiki page: the list of stored versions, a line
  diff between any two of them and a restore that re-applies an old version.
  The server keeps up to MAX_WIKI_REVISIONS_KEPT versions per page; older
  ones are pruned and simply do not appear here.
-->
<script lang="ts">
  import { wiki, type WikiPage, type WikiRevision, type WikiRevisionMeta } from '$lib/stores/wiki';
  import { collapseUnchanged, diffLines, type DiffRow } from '$lib/wiki/diff';
  import { dialogs } from '$lib/stores/dialogs';

  interface Props {
    channelId: number;
    /** The page as the viewer currently has it; its revision is the restore base. */
    page: WikiPage;
    canEdit?: boolean;
    onClose: () => void;
    /** A restore landed; the parent leaves the history and reloads the page. */
    onRestored?: () => void;
  }

  let { channelId, page, canEdit = false, onClose, onRestored = () => {} }: Props = $props();

  /** Lines of unchanged context kept around each change before folding. */
  const CONTEXT_LINES = 3;

  let revisions: WikiRevisionMeta[] = $state([]);
  let loading = $state(true);
  let failed = $state(false);
  /** The revision the reader picked, if it is still in the list. */
  let selectedChoice: number | null = $state(null);
  /** The revision compared against; null means "the one just before it". */
  let baseChoice: number | null = $state(null);
  let showAll = $state(false);
  let restoring = $state(false);
  /** Revision bodies, fetched once each and kept for as long as this is open. */
  let bodies: Map<number, WikiRevision | null> = $state(new Map());
  let loadingDiff = $state(false);
  let diffFailed = $state(false);
  /** Bodies currently being fetched, so a re-run never asks twice. */
  const inflight = new Map<number, Promise<WikiRevision | null>>();

  async function loadHistory() {
    loading = true;
    failed = false;
    try {
      /* Reads of reactive state before this await are what the reloading
         effect below tracks — keep it to the page being viewed. */
      revisions = await wiki.history(channelId, page.slug);
    } catch {
      failed = true;
    } finally {
      loading = false;
    }
  }

  /**
   * Fetch a revision body once, sharing the cache between both diff sides.
   * Filling the cache re-runs the effect below while the other side may
   * still be in flight, which is why requests are deduplicated here too.
   */
  function body(revision: number): Promise<WikiRevision | null> {
    const cached = bodies.get(revision);
    if (cached !== undefined) return Promise.resolve(cached);
    const existing = inflight.get(revision);
    if (existing) return existing;
    const pending = wiki
      .getRevision(channelId, page.slug, revision)
      .then((loaded) => {
        // A fresh Map is what tells the deriveds below that it changed.
        bodies = new Map(bodies).set(revision, loaded);
        return loaded;
      })
      .finally(() => inflight.delete(revision));
    inflight.set(revision, pending);
    return pending;
  }

  function selectRevision(revision: number) {
    if (revision === selected) return;
    selectedChoice = revision;
    baseChoice = null;
    showAll = false;
  }

  async function restore(revision: number) {
    const confirmed = await dialogs.confirm({
      title: 'Restore this version?',
      message: `Revision ${revision} becomes the newest version of "${page.title}". Nothing is lost — the current version stays in the history.`,
      confirmLabel: 'Restore'
    });
    if (!confirmed) return;
    restoring = true;
    try {
      const result = await wiki.restore(channelId, page.slug, revision, page.revision);
      if (result.ok) {
        onRestored();
      } else {
        // Someone saved while the history was open; restoring on the stale
        // base would have overwritten them without either side noticing.
        await dialogs.alert({
          title: 'Wiki',
          message: `${result.current.updatedBy || 'Someone'} saved revision ${result.current.revision} in the meantime. Check the diff against it and try again.`
        });
        await loadHistory();
      }
    } catch {
      await dialogs.alert({
        title: 'Wiki',
        message: 'The version could not be restored. Please try again.'
      });
    } finally {
      restoring = false;
    }
  }

  function formatTimestamp(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    return `${(bytes / 1024).toFixed(1)} kB`;
  }

  /* The selection is derived rather than reconciled after every load: a
     revision that has been pruned away, or a page switched underneath the
     view, then simply falls back to the newest version. */
  let selected = $derived(
    selectedChoice !== null && revisions.some((r) => r.revision === selectedChoice)
      ? selectedChoice
      : (revisions[0]?.revision ?? null)
  );
  let selectedMeta = $derived(revisions.find((r) => r.revision === selected) ?? null);
  /** The revision immediately older than the selected one, if it is kept. */
  let previousRevision = $derived.by(() => {
    const index = revisions.findIndex((r) => r.revision === selected);
    // The list is newest first, so the next entry is the older version.
    return index >= 0 ? (revisions[index + 1]?.revision ?? null) : null;
  });
  let baseRevision = $derived(
    baseChoice !== null && revisions.some((r) => r.revision === baseChoice)
      ? baseChoice
      : previousRevision
  );
  let isCurrent = $derived(selected !== null && selected === revisions[0]?.revision);

  // Fetch whichever bodies the current comparison needs. Both sides go
  // through the same cache, so switching the base only costs one request.
  $effect(() => {
    const target = selected;
    const base = baseRevision;
    if (target === null) return;
    loadingDiff = true;
    void Promise.all([body(target), base === null ? null : body(base)])
      .then(() => {
        diffFailed = false;
      })
      .catch(() => {
        // Timed out or disconnected — say so rather than claiming the
        // revision is gone, which is what a null body would look like.
        diffFailed = true;
      })
      .finally(() => {
        loadingDiff = false;
      });
  });

  let targetBody = $derived(selected === null ? null : (bodies.get(selected) ?? null));
  let baseBody = $derived(baseRevision === null ? null : (bodies.get(baseRevision) ?? null));
  /** A first revision has nothing before it: everything in it was added. */
  let baseText = $derived(baseRevision === null ? '' : (baseBody?.body ?? null));
  let diff = $derived.by(() => {
    if (targetBody === null || baseText === null) return null;
    return diffLines(baseText, targetBody.body);
  });
  let rows: DiffRow[] = $derived.by(() => {
    if (!diff) return [];
    return showAll ? diff.lines : collapseUnchanged(diff.lines, CONTEXT_LINES);
  });
  let hasFoldedLines = $derived(rows.some((row) => row.op === 'gap'));
  let titleChanged = $derived(
    targetBody !== null && baseBody !== null && targetBody.title !== baseBody.title
  );

  // Reload whenever the page — or a save on it — changes underneath.
  $effect(() => {
    void page.slug;
    void page.revision;
    void loadHistory();
  });
</script>

<div class="history">
  <aside class="revisions">
    <div class="revisions-header">
      <h3>History</h3>
      <button class="btn btn-ghost" onclick={onClose}>Back to page</button>
    </div>
    {#if loading}
      <p class="note">Loading history…</p>
    {:else if failed}
      <p class="note">The history could not be loaded.</p>
    {:else if revisions.length === 0}
      <p class="note">No stored revisions.</p>
    {:else}
      <ul>
        {#each revisions as revision (revision.revision)}
          <li>
            <button
              class="revision"
              class:active={revision.revision === selected}
              class:base={revision.revision === baseRevision}
              onclick={() => selectRevision(revision.revision)}
            >
              <span class="revision-line">
                <span class="revision-number">rev {revision.revision}</span>
                {#if revision.revision === revisions[0].revision}
                  <span class="tag">current</span>
                {:else if revision.revision === baseRevision}
                  <span class="tag tag-base">compared</span>
                {/if}
              </span>
              <span class="revision-meta">
                {revision.author || 'unknown'} · {formatTimestamp(revision.createdAt)}
              </span>
              <span class="revision-meta">{revision.title} · {formatBytes(revision.bytes)}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </aside>

  <section class="diff-pane">
    {#if selectedMeta}
      {@const meta = selectedMeta}
      <div class="diff-header">
        <div class="diff-title">
          <h4>Revision {meta.revision}</h4>
          <span class="diff-sub">
            by {meta.author || 'unknown'} · {formatTimestamp(meta.createdAt)}
          </span>
        </div>
        <div class="diff-actions">
          <label class="compare">
            <span>Compare with</span>
            <select
              value={baseChoice === null ? '' : String(baseChoice)}
              onchange={(e) => {
                const value = e.currentTarget.value;
                baseChoice = value === '' ? null : Number(value);
                showAll = false;
              }}
            >
              <option value="">
                {previousRevision === null ? 'nothing (first kept version)' : 'previous revision'}
              </option>
              {#each revisions.filter((r) => r.revision !== meta.revision) as option (option.revision)}
                <option value={String(option.revision)}>rev {option.revision}</option>
              {/each}
            </select>
          </label>
          {#if canEdit && !isCurrent}
            <button
              class="btn btn-primary"
              disabled={restoring}
              onclick={() => restore(meta.revision)}
            >
              {restoring ? 'Restoring…' : 'Restore this version'}
            </button>
          {/if}
        </div>
      </div>

      {#if diff}
        <div class="diff-summary">
          <span class="added">+{diff.added}</span>
          <span class="removed">−{diff.removed}</span>
          <span class="against">
            {baseRevision === null
              ? 'against an empty page'
              : `rev ${baseRevision} → rev ${meta.revision}`}
          </span>
          {#if titleChanged}
            <span class="against">title: “{baseBody?.title}” → “{targetBody?.title}”</span>
          {/if}
          {#if !diff.exact}
            <span class="against warn">
              too different to align line by line — shown as a full replacement
            </span>
          {/if}
          {#if showAll}
            <button class="link-btn" onclick={() => (showAll = false)}>Collapse unchanged</button>
          {:else if hasFoldedLines}
            <button class="link-btn" onclick={() => (showAll = true)}>Show whole page</button>
          {/if}
        </div>
        {#if diff.added === 0 && diff.removed === 0}
          <p class="note">These two versions are identical.</p>
        {:else}
          <div class="diff-body">
            {#each rows as row, index (index)}
              {#if row.op === 'gap'}
                <button class="gap" onclick={() => (showAll = true)}>
                  … {row.count} unchanged {row.count === 1 ? 'line' : 'lines'}
                </button>
              {:else}
                <div class="line {row.op}">
                  <span class="gutter">{row.oldLine ?? ''}</span>
                  <span class="gutter">{row.newLine ?? ''}</span>
                  <span class="sign">{row.op === 'add' ? '+' : row.op === 'remove' ? '−' : ' '}</span>
                  <span class="text">{row.text || ' '}</span>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
      {:else if loadingDiff}
        <p class="note">Loading diff…</p>
      {:else if diffFailed}
        <p class="note">The diff could not be loaded.</p>
      {:else}
        <p class="note">This revision is no longer stored.</p>
      {/if}
    {:else if !loading}
      <p class="note">Select a revision to see what changed.</p>
    {/if}
  </section>
</div>

<style>
  .history {
    display: flex;
    flex: 1;
    min-height: 0;
    gap: var(--space-3);
  }

  .revisions {
    display: flex;
    flex-direction: column;
    width: 16rem;
    flex-shrink: 0;
    overflow-y: auto;
    border-right: 1px solid var(--color-surface-outline);
    padding-right: var(--space-2);
  }

  .revisions-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  .revisions-header h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .revisions ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .revision {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    text-align: left;
    border: none;
    border-left: 2px solid transparent;
    background: transparent;
    color: var(--color-on-surface-variant);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .revision:hover,
  .revision:focus-visible {
    background: var(--color-surface-raised);
    outline: none;
  }

  .revision.active {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .revision.base:not(.active) {
    border-left-color: var(--color-outline-strong);
  }

  .revision-line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .revision-number {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .tag {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 0 var(--space-1);
    border-radius: var(--radius-sm);
    background: var(--color-primary-container);
    color: var(--color-on-surface);
  }

  .tag-base {
    background: var(--color-surface-outline);
    color: var(--color-muted);
  }

  .revision-meta {
    font-size: var(--text-xs);
    color: var(--color-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diff-pane {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    gap: var(--space-2);
  }

  .diff-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .diff-title h4 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .diff-sub {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .diff-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .compare {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .compare select {
    background: var(--color-surface);
    color: var(--color-on-surface);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm);
    padding: var(--space-1) var(--space-2);
    font-size: var(--text-sm);
  }

  .diff-summary {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .added {
    color: var(--color-success);
  }

  .removed {
    color: var(--color-error);
  }

  .against {
    color: var(--color-muted);
  }

  .warn {
    color: var(--color-warning);
  }

  .link-btn {
    border: none;
    background: none;
    padding: 0;
    color: var(--color-primary);
    font-size: var(--text-xs);
    font-family: inherit;
    cursor: pointer;
    text-decoration: underline;
  }

  .diff-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    padding: var(--space-1) 0;
  }

  .line {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.5;
    padding: 0 var(--space-2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .line.add {
    background: color-mix(in srgb, var(--color-success) 18%, transparent);
  }

  .line.remove {
    background: color-mix(in srgb, var(--color-error) 18%, transparent);
  }

  .gutter {
    flex-shrink: 0;
    width: 3ch;
    text-align: right;
    color: var(--color-muted);
    user-select: none;
  }

  .sign {
    flex-shrink: 0;
    width: 1ch;
    user-select: none;
  }

  .line.add .sign {
    color: var(--color-success);
  }

  .line.remove .sign {
    color: var(--color-error);
  }

  .text {
    flex: 1;
    min-width: 0;
    color: var(--color-on-surface);
  }

  .gap {
    display: block;
    width: 100%;
    border: none;
    border-top: 1px solid var(--color-surface-outline);
    border-bottom: 1px solid var(--color-surface-outline);
    background: var(--color-surface-raised);
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    padding: var(--space-1) var(--space-2);
    margin: var(--space-1) 0;
    text-align: left;
    cursor: pointer;
  }

  .gap:hover {
    color: var(--color-on-surface-variant);
  }

  .note {
    margin: 0;
    padding: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-sm);
    font-style: italic;
  }
</style>
