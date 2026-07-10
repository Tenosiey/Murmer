import { get } from 'svelte/store';
import { selectedServer } from './stores/servers';

export interface LinkPreviewData {
  url: string;
  siteName?: string;
  title?: string;
  description?: string;
  image?: string;
}

/** Convert the selected WebSocket server URL into its HTTP API base. */
function serverHttpBase(): string | null {
  const selected = get(selectedServer);
  if (!selected) return null;
  try {
    const u = new URL(selected);
    u.protocol = u.protocol.replace('ws', 'http');
    if (u.pathname.endsWith('/ws')) u.pathname = u.pathname.slice(0, -3);
    return u.toString().replace(/\/$/, '');
  } catch {
    return null;
  }
}

const previewCache = new Map<string, Promise<LinkPreviewData | null>>();

/**
 * Fetch OpenGraph metadata for a URL via the server's /link-preview endpoint.
 * Resolves to null when the server has no preview for the link. Results are
 * cached for the lifetime of the app so repeated renders of the same link
 * don't refetch.
 */
export function fetchLinkPreview(url: string): Promise<LinkPreviewData | null> {
  const cached = previewCache.get(url);
  if (cached) return cached;

  const base = serverHttpBase();
  if (!base) return Promise.resolve(null);

  const promise = fetch(`${base}/link-preview?url=${encodeURIComponent(url)}`)
    .then(async (res) => {
      if (!res.ok) return null;
      const data = (await res.json()) as LinkPreviewData;
      return data && (data.title || data.description) ? data : null;
    })
    .catch(() => {
      // Don't cache network failures so a reconnect can retry.
      previewCache.delete(url);
      return null;
    });
  previewCache.set(url, promise);
  return promise;
}

export function extractLinks(text: string | undefined | null): string[] {
  if (!text) return [];
  const urlPattern = /https?:\/\/[\w.-]+(?:\/[\w\-./?%&=+#@~:,;!]*)?/gi;
  const results = new Set<string>();
  let match: RegExpExecArray | null;
  while ((match = urlPattern.exec(text)) !== null) {
    let url = match[0];
    // Trim common trailing punctuation
    url = url.replace(/[).,!?"'\]]+$/g, '');
    try {
      const parsed = new URL(url);
      if (parsed.protocol === 'http:' || parsed.protocol === 'https:') {
        results.add(parsed.toString());
      }
    } catch (error) {
      // ignore invalid URLs
    }
  }
  return [...results];
}
