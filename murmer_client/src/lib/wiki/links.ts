/**
 * Behaviour for rendered `[[wikilinks]]`: click navigation and
 * missing-page ("create stub") styling. The Markdown renderer only emits
 * inert anchors with data attributes; this action makes them live.
 */

import { wiki, type WikiLinkTarget } from '../stores/wiki';

export type { ParsedWikiTarget } from './slug';
export { slugify, parseWikiTarget } from './slug';

/** A click on a wikilink; `channel` is null for same-channel links. */
export type WikiNavigation = { channel: string | null; slug: string };

export type WikilinksOptions = {
  /** Name of the channel the content belongs to (for `[[page]]` links). */
  channelName: string;
  /** Called on wikilink clicks; omit to render links inert (editor preview). */
  onNavigate?: (nav: WikiNavigation) => void;
};

const MARK_DEBOUNCE_MS = 100;

/**
 * Svelte action: attach to any container whose HTML may contain rendered
 * wikilinks. Delegates clicks, and resolves link targets against the server
 * to tag missing pages with the `wikilink-missing` class. Re-marks when the
 * container's content or the wiki index changes.
 */
export function wikilinks(node: HTMLElement, options: WikilinksOptions) {
  let opts = options;
  let markTimer: ReturnType<typeof setTimeout> | null = null;
  let destroyed = false;

  function handleClick(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    const anchor = target?.closest?.('a.wikilink');
    if (!anchor || !node.contains(anchor)) return;
    event.preventDefault();
    event.stopPropagation();
    const slug = anchor.getAttribute('data-wiki-slug');
    if (!slug) return;
    const channel = anchor.getAttribute('data-wiki-channel') || null;
    opts.onNavigate?.({ channel, slug });
  }

  async function markMissing() {
    const anchors = Array.from(
      node.querySelectorAll<HTMLAnchorElement>('a.wikilink[data-wiki-slug]')
    );
    if (anchors.length === 0) return;

    const targets: WikiLinkTarget[] = anchors.map((anchor) => ({
      channel: anchor.getAttribute('data-wiki-channel') || opts.channelName,
      slug: anchor.getAttribute('data-wiki-slug') ?? ''
    }));

    try {
      const resolved = await wiki.resolveLinks(targets);
      if (destroyed) return;
      anchors.forEach((anchor, i) => {
        const exists = resolved.get(`${targets[i].channel}/${targets[i].slug}`);
        if (exists === false) {
          anchor.classList.add('wikilink-missing');
          anchor.title = 'This page does not exist yet';
        } else if (exists === true) {
          anchor.classList.remove('wikilink-missing');
          anchor.removeAttribute('title');
        }
      });
    } catch {
      // Offline or timed out — leave the links unmarked; the next content
      // or index change retries.
    }
  }

  function scheduleMark() {
    if (markTimer !== null) clearTimeout(markTimer);
    markTimer = setTimeout(() => {
      markTimer = null;
      void markMissing();
    }, MARK_DEBOUNCE_MS);
  }

  // Content re-renders ({@html} swaps) and wiki index changes (pages
  // created/renamed/deleted elsewhere) both invalidate the marks. Class and
  // title changes made above are attribute mutations, which this observer
  // deliberately does not watch — no feedback loop.
  const observer = new MutationObserver(scheduleMark);
  observer.observe(node, { childList: true, subtree: true });
  const unsubscribe = wiki.subscribe(scheduleMark);
  node.addEventListener('click', handleClick);
  void markMissing();

  return {
    update(next: WikilinksOptions) {
      opts = next;
      scheduleMark();
    },
    destroy() {
      destroyed = true;
      if (markTimer !== null) clearTimeout(markTimer);
      observer.disconnect();
      unsubscribe();
      node.removeEventListener('click', handleClick);
    }
  };
}
