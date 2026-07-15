import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message } from '../types';

/** Metadata carried in `wiki-index` snapshots (no body). */
export type WikiPageMeta = {
  slug: string;
  title: string;
  revision: number;
  updatedBy: string;
  updatedAt: string;
};

/** A full wiki page including its Markdown body. */
export type WikiPage = WikiPageMeta & {
  body: string;
  author: string;
  createdAt: string;
};

/** Result of a save attempt: saved, or conflicted with the current page. */
export type WikiSaveResult =
  | { ok: true; revision: number }
  | { ok: false; current: WikiPage };

/** A `[[channel/slug]]` link target; `channel` is the channel name. */
export type WikiLinkTarget = { channel: string; slug: string };

const REQUEST_TIMEOUT_MS = 5000;
/** Server cap for links per wiki-resolve request. */
const MAX_RESOLVE_LINKS = 50;

type Pending<T> = {
  resolve: (value: T) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

function parseMeta(raw: any): WikiPageMeta | null {
  if (!raw || typeof raw !== 'object') return null;
  if (typeof raw.slug !== 'string' || typeof raw.title !== 'string') return null;
  return {
    slug: raw.slug,
    title: raw.title,
    revision: typeof raw.revision === 'number' ? raw.revision : 0,
    updatedBy: typeof raw.updatedBy === 'string' ? raw.updatedBy : '',
    updatedAt: typeof raw.updatedAt === 'string' ? raw.updatedAt : ''
  };
}

function parsePage(raw: any): WikiPage | null {
  const meta = parseMeta(raw);
  if (!meta) return null;
  return {
    ...meta,
    body: typeof raw.body === 'string' ? raw.body : '',
    author: typeof raw.author === 'string' ? raw.author : '',
    createdAt: typeof raw.createdAt === 'string' ? raw.createdAt : ''
  };
}

function linkKey(target: WikiLinkTarget): string {
  return `${target.channel}/${target.slug}`;
}

function createWikiStore() {
  /** Per-channel page metadata, replaced wholesale by every `wiki-index`. */
  const { subscribe, update, set } = writable<Record<number, WikiPageMeta[]>>({});

  let requestIdCounter = 1;
  const pendingPages = new Map<number, Pending<WikiPage | null>>();
  const pendingResolves = new Map<number, Pending<Map<string, boolean>>>();
  const pendingSaves = new Map<number, Pending<WikiSaveResult>>();
  /** Session cache of link existence, dropped whenever any index changes. */
  const resolveCache = new Map<string, boolean>();

  function takePending<T>(map: Map<number, Pending<T>>, raw: any): Pending<T> | null {
    const requestId = Number(raw?.requestId);
    if (Number.isNaN(requestId)) return null;
    const pending = map.get(requestId);
    if (!pending) return null;
    map.delete(requestId);
    clearTimeout(pending.timeout);
    return pending;
  }

  function request<T>(map: Map<number, Pending<T>>, payload: Record<string, unknown>): Promise<T> {
    const requestId = requestIdCounter++;
    return new Promise<T>((resolve, reject) => {
      const timeout = setTimeout(() => {
        if (map.delete(requestId)) {
          reject(new Error('Wiki request timed out'));
        }
      }, REQUEST_TIMEOUT_MS);
      map.set(requestId, { resolve, reject, timeout });
      chat.sendRaw({ ...payload, requestId });
    });
  }

  function rejectAll(reason: string) {
    for (const map of [pendingPages, pendingResolves, pendingSaves] as Map<
      number,
      Pending<any>
    >[]) {
      for (const pending of map.values()) {
        clearTimeout(pending.timeout);
        pending.reject(new Error(reason));
      }
      map.clear();
    }
  }

  function reset() {
    set({});
    resolveCache.clear();
    rejectAll('Connection reset');
  }

  chat.on('wiki-index', (msg: Message) => {
    const raw = msg as any;
    if (typeof raw.channelId !== 'number' || !Array.isArray(raw.pages)) return;
    const pages = raw.pages
      .map(parseMeta)
      .filter((p: WikiPageMeta | null): p is WikiPageMeta => p !== null);
    update((index) => ({ ...index, [raw.channelId]: pages }));
    // Pages may have been created, renamed or deleted; cached link
    // existence can no longer be trusted.
    resolveCache.clear();
  });

  chat.on('wiki-page', (msg: Message) => {
    const raw = msg as any;
    const pending = takePending(pendingPages, raw);
    if (!pending) return;
    pending.resolve(parsePage(raw.page));
  });

  chat.on('wiki-resolved', (msg: Message) => {
    const raw = msg as any;
    const pending = takePending(pendingResolves, raw);
    if (!pending) return;
    const result = new Map<string, boolean>();
    if (Array.isArray(raw.results)) {
      for (const entry of raw.results) {
        if (entry && typeof entry.channel === 'string' && typeof entry.slug === 'string') {
          const exists = entry.exists === true;
          const key = linkKey(entry);
          result.set(key, exists);
          resolveCache.set(key, exists);
        }
      }
    }
    pending.resolve(result);
  });

  chat.on('wiki-saved', (msg: Message) => {
    const raw = msg as any;
    const pending = takePending(pendingSaves, raw);
    if (!pending) return;
    pending.resolve({ ok: true, revision: typeof raw.revision === 'number' ? raw.revision : 0 });
  });

  chat.on('wiki-conflict', (msg: Message) => {
    const raw = msg as any;
    const pending = takePending(pendingSaves, raw);
    if (!pending) return;
    const current = parsePage(raw.page);
    if (current) {
      pending.resolve({ ok: false, current });
    } else {
      pending.reject(new Error('Malformed wiki conflict response'));
    }
  });

  chat.on('channel-remove', (msg: Message) => {
    const id = (msg as any).channelId;
    if (typeof id !== 'number') return;
    update((index) => {
      if (!(id in index)) return index;
      const next = { ...index };
      delete next[id];
      return next;
    });
    resolveCache.clear();
  });

  // A fresh connection starts from a clean slate; the server re-sends the
  // wiki index on every channel join.
  connection.subscribe((state) => {
    if (state === 'connecting' || state === 'idle') {
      reset();
    }
  });

  /** Fetch a full page; resolves `null` when the page does not exist. */
  function getPage(channelId: number, slug: string): Promise<WikiPage | null> {
    return request(pendingPages, { type: 'wiki-get', channelId, slug });
  }

  /**
   * Batched existence check for wiki links. Deduplicates, serves cached
   * results and chunks the rest to the server limit.
   */
  async function resolveLinks(links: WikiLinkTarget[]): Promise<Map<string, boolean>> {
    const result = new Map<string, boolean>();
    const missing = new Map<string, WikiLinkTarget>();
    for (const link of links) {
      const key = linkKey(link);
      const cached = resolveCache.get(key);
      if (cached !== undefined) {
        result.set(key, cached);
      } else if (!missing.has(key)) {
        missing.set(key, link);
      }
    }

    const remaining = [...missing.values()];
    for (let i = 0; i < remaining.length; i += MAX_RESOLVE_LINKS) {
      const chunk = remaining.slice(i, i + MAX_RESOLVE_LINKS);
      const resolved = await request(pendingResolves, {
        type: 'wiki-resolve',
        links: chunk
      });
      for (const [key, exists] of resolved) {
        result.set(key, exists);
      }
    }
    return result;
  }

  /**
   * Save a page. Resolves `{ok: false, current}` when someone else saved a
   * newer revision first; server errors surface via the error toast path.
   */
  function save(
    channelId: number,
    slug: string,
    title: string,
    body: string,
    expectedRevision: number
  ): Promise<WikiSaveResult> {
    return request(pendingSaves, {
      type: 'wiki-update',
      channelId,
      slug,
      title,
      body,
      expectedRevision
    });
  }

  /** Create a page; the store updates when the index broadcasts back. */
  function createPage(channelId: number, slug: string, title: string, body = '') {
    chat.sendRaw({ type: 'wiki-create', channelId, slug, title, body });
  }

  /** Delete a page; observed via the index broadcast. */
  function deletePage(channelId: number, slug: string) {
    chat.sendRaw({ type: 'wiki-delete', channelId, slug });
  }

  /** Rename (re-slug) a page; observed via the index broadcast. */
  function renamePage(channelId: number, slug: string, newSlug: string) {
    chat.sendRaw({ type: 'wiki-rename', channelId, slug, newSlug });
  }

  return {
    subscribe,
    getPage,
    resolveLinks,
    save,
    createPage,
    deletePage,
    renamePage,
    reset
  };
}

export const wiki = createWikiStore();
