/**
 * Wiki slug helpers shared by the Markdown renderer and the wiki UI.
 * Kept dependency-free so `markdown.ts` can import it without dragging in
 * the store graph.
 */

/** Server-side slug length limit (mirrors MAX_WIKI_SLUG_LENGTH). */
export const MAX_WIKI_SLUG_LENGTH = 64;

/**
 * Derive a canonical slug from free text: lowercase alphanumerics with
 * single dashes, mirroring the server's `validate_wiki_slug` rules.
 */
export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, MAX_WIKI_SLUG_LENGTH)
    .replace(/-+$/, '');
}

/** A parsed `[[target]]` — `channel` is null for same-channel links. */
export type ParsedWikiTarget = {
  channel: string | null;
  slug: string;
  label: string;
};

/**
 * Parse the inside of a `[[target]]` or `[[target|label]]` wiki link.
 * `channel/page` addresses another channel by name; the page part is
 * slugified so `[[Getting Started]]` links to `getting-started`.
 */
export function parseWikiTarget(target: string, label?: string): ParsedWikiTarget | null {
  const trimmed = target.trim();
  if (!trimmed) return null;

  let channel: string | null = null;
  let pagePart = trimmed;
  const slashIndex = trimmed.indexOf('/');
  if (slashIndex >= 0) {
    channel = trimmed.slice(0, slashIndex).trim();
    pagePart = trimmed.slice(slashIndex + 1).trim();
    if (!channel) return null;
  }

  const slug = slugify(pagePart);
  if (!slug) return null;

  const fallbackLabel = pagePart || slug;
  return { channel, slug, label: (label ?? fallbackLabel).trim() || slug };
}
