import { get } from 'svelte/store';
import { selectedServer } from './stores/servers';
import { httpBaseFromWs } from './server-url';
import { parseMessageLink } from './message-link';

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
    return httpBaseFromWs(selected);
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
      if (!data || !(data.title || data.description)) return null;
      // The server inlines the image. A URL would be fetched by every reader
      // from a host the link's poster chose, reporting their IP address.
      const inlined = typeof data.image === 'string' && data.image.startsWith('data:image/');
      return { ...data, image: inlined ? data.image : undefined };
    })
    .catch(() => {
      // Don't cache network failures so a reconnect can retry.
      previewCache.delete(url);
      return null;
    });
  previewCache.set(url, promise);
  return promise;
}

/**
 * Resolve a Giphy link to a directly embeddable .gif URL, or null when the
 * link is not a Giphy GIF. Only giphy.com hosts are accepted so this path
 * can't be used to inline arbitrary third-party images.
 */
export function giphyGifUrl(url: string): string | null {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return null;
    const host = parsed.hostname.toLowerCase();
    if (host !== 'giphy.com' && !host.endsWith('.giphy.com')) return null;
    // Direct media links, e.g. https://media2.giphy.com/media/<path>/giphy.gif
    if (host !== 'giphy.com' && /\.gif$/i.test(parsed.pathname)) {
      return parsed.toString();
    }
    // Share/page links, e.g. https://giphy.com/gifs/reaction-slug-RK51HqhhEx2bYAjjti
    const match = parsed.pathname.match(/^\/(?:gifs|stickers)\/(?:[\w-]*-)?(\w+)\/?$/);
    if (match) {
      return `https://media.giphy.com/media/${match[1]}/giphy.gif`;
    }
    return null;
  } catch {
    return null;
  }
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
      // A message link jumps inside Murmer; previewing it would only show
      // the web client's empty page.
      if (
        (parsed.protocol === 'http:' || parsed.protocol === 'https:') &&
        !parseMessageLink(url)
      ) {
        results.add(parsed.toString());
      }
    } catch (error) {
      // ignore invalid URLs
    }
  }
  return [...results];
}
