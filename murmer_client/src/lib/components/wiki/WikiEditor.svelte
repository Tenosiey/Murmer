<!--
  Wiki page editor: title field plus a split Markdown editor with a live,
  debounced preview. Saves use the revision compare-and-swap; a conflict
  (someone else saved first) shows a banner instead of silently overwriting.
-->
<script lang="ts">
  import { renderMarkdown } from '$lib/markdown';
  import { emojifyHtml } from '$lib/emoji';
  import { customEmojis } from '$lib/stores/customEmojis';
  import { selectedServer } from '$lib/stores/servers';
  import { httpBaseFromWs } from '$lib/server-url';
  import { wiki, type WikiPage } from '$lib/stores/wiki';
  import { wikilinks } from '$lib/wiki/links';
  import { dialogs } from '$lib/stores/dialogs';

  export let channelId: number;
  export let channelName: string;
  export let page: WikiPage;
  export let onSaved: () => void;
  export let onCancel: () => void;
  /** Bound by the parent to guard page switches while edits are unsaved. */
  export let dirty = false;

  /** Server-side body cap (mirrors MAX_WIKI_BODY_BYTES). */
  const MAX_BODY_BYTES = 100_000;
  const PREVIEW_DEBOUNCE_MS = 150;

  let title = page.title;
  let body = page.body;
  /** Revision this edit is based on; the save CAS targets it. */
  let baseRevision = page.revision;
  let saving = false;
  /** Set when a save came back conflicted; carries the winning version. */
  let conflict: WikiPage | null = null;

  $: dirty = title !== page.title || body !== page.body;
  $: httpBase = $selectedServer ? httpBaseFromWs($selectedServer) : '';

  // Debounce the preview so typing stays responsive on large pages.
  let previewBody = page.body;
  let previewTimer: ReturnType<typeof setTimeout> | null = null;
  $: {
    void body;
    if (previewTimer !== null) clearTimeout(previewTimer);
    previewTimer = setTimeout(() => {
      previewBody = body;
    }, PREVIEW_DEBOUNCE_MS);
  }
  $: previewHtml = emojifyHtml(renderMarkdown(previewBody), $customEmojis, httpBase);

  $: bodyBytes = new TextEncoder().encode(body).length;
  $: tooLarge = bodyBytes > MAX_BODY_BYTES;

  // Warn before saving when the index shows someone else already saved a
  // newer revision of this page.
  $: latestMeta = ($wiki[channelId] ?? []).find((p) => p.slug === page.slug);
  $: remoteRevision = latestMeta?.revision ?? baseRevision;
  $: conflictUser = conflict?.updatedBy ?? (remoteRevision > baseRevision ? latestMeta?.updatedBy : null) ?? null;
  $: hasConflict = conflict !== null || remoteRevision > baseRevision;

  async function save() {
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      await dialogs.alert({ title: 'Wiki', message: 'The page needs a title.' });
      return;
    }
    if (tooLarge) {
      await dialogs.alert({ title: 'Wiki', message: 'This page is too large to save.' });
      return;
    }
    saving = true;
    try {
      const result = await wiki.save(channelId, page.slug, trimmedTitle, body, baseRevision);
      if (result.ok) {
        onSaved();
      } else {
        conflict = result.current;
      }
    } catch {
      await dialogs.alert({
        title: 'Wiki',
        message: 'The page could not be saved. Please try again.'
      });
    } finally {
      saving = false;
    }
  }

  /** Replace the draft with the newest server version. */
  async function loadNewest() {
    try {
      const current = conflict ?? (await wiki.getPage(channelId, page.slug));
      if (current) {
        title = current.title;
        body = current.body;
        baseRevision = current.revision;
      }
      conflict = null;
    } catch {
      await dialogs.alert({
        title: 'Wiki',
        message: 'The newest version could not be loaded. Please try again.'
      });
    }
  }

  /** Keep the draft; the next save knowingly overwrites the newer version. */
  function keepEditing() {
    baseRevision = conflict?.revision ?? remoteRevision;
    conflict = null;
  }
</script>

<div class="editor">
  <input
    class="field title-field"
    bind:value={title}
    placeholder="Page title"
    maxlength="100"
    aria-label="Page title"
  />

  <div class="panes">
    <textarea
      class="field body-field"
      bind:value={body}
      placeholder="Write Markdown… link other pages with [[page]] or [[channel/page]]"
      aria-label="Page content"
      spellcheck="false"
    ></textarea>
    <div class="preview wiki-body" use:wikilinks={{ channelName }} aria-label="Preview">
      {@html previewHtml}
    </div>
  </div>

  {#if hasConflict}
    <div class="conflict-banner" role="alert">
      <span>
        {#if conflictUser}
          {conflictUser} saved a newer version of this page.
        {:else}
          A newer version of this page was saved.
        {/if}
      </span>
      <div class="conflict-actions">
        <button class="btn btn-ghost" on:click={loadNewest}>Load newest (discard my edits)</button>
        <button class="btn btn-ghost" on:click={keepEditing}>Keep editing</button>
      </div>
    </div>
  {/if}

  <div class="footer">
    <span class="byte-counter" class:over={tooLarge}>
      {bodyBytes.toLocaleString()} / {MAX_BODY_BYTES.toLocaleString()} bytes
    </span>
    <div class="footer-actions">
      <button class="btn btn-ghost" on:click={onCancel} disabled={saving}>Cancel</button>
      <button class="btn btn-primary" on:click={save} disabled={saving || tooLarge}>
        {saving ? 'Saving…' : hasConflict ? 'Overwrite' : 'Save'}
      </button>
    </div>
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    flex: 1;
    min-height: 0;
  }

  .title-field {
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .panes {
    display: flex;
    gap: var(--space-3);
    flex: 1;
    min-height: 0;
  }

  .body-field {
    flex: 1;
    min-width: 0;
    resize: none;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: 1.55;
    padding: var(--space-3);
  }

  .preview {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--space-3);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .conflict-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-warning) 45%, transparent);
    background: color-mix(in srgb, var(--color-warning) 12%, transparent);
    color: var(--color-on-surface);
    font-size: var(--text-sm);
  }

  .conflict-actions {
    display: flex;
    gap: var(--space-2);
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .byte-counter {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .byte-counter.over {
    color: var(--color-error);
  }

  .footer-actions {
    display: flex;
    gap: var(--space-2);
  }

  @media (max-width: 900px) {
    .panes {
      flex-direction: column;
    }
  }
</style>
