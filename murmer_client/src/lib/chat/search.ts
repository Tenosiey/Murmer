import type { WikiSearchHit } from '../types';

/** Longest snippet rendered in the result list; the server already excerpts. */
const MAX_SNIPPET_CHARS = 160;

/**
 * Parse the `pages` array of a `search-results` frame.
 *
 * Wiki hits arrive on the same frame as the message hits, so a malformed or
 * missing entry must cost only that entry: anything without a usable slug and
 * title is dropped rather than rendered as a blank row. Snippets are collapsed
 * to single-line plain text — they are raw Markdown from the page body and are
 * shown as text, never rendered.
 */
export function parseWikiSearchHits(raw: unknown): WikiSearchHit[] {
  if (!Array.isArray(raw)) return [];
  const hits: WikiSearchHit[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== 'object') continue;
    const page = entry as Record<string, unknown>;
    const slug = typeof page.slug === 'string' ? page.slug : '';
    const title = typeof page.title === 'string' ? page.title.trim() : '';
    if (!slug || !title) continue;
    hits.push({
      slug,
      title,
      snippet: normalizeSnippet(page.snippet),
      updatedBy: typeof page.updatedBy === 'string' ? page.updatedBy : '',
      updatedAt: typeof page.updatedAt === 'string' ? page.updatedAt : ''
    });
  }
  return hits;
}

/** Collapse a page excerpt to one bounded line of text. */
function normalizeSnippet(raw: unknown): string {
  if (typeof raw !== 'string') return '';
  const collapsed = raw.replace(/\s+/g, ' ').trim();
  return collapsed.length > MAX_SNIPPET_CHARS
    ? `${collapsed.slice(0, MAX_SNIPPET_CHARS - 1)}…`
    : collapsed;
}

/** The `filters` field of a `search-history` frame. */
export interface SearchFilters {
  /** Account name of the author; never a display name. */
  from?: string;
  hasFile?: boolean;
  /** RFC 3339 bounds: before is exclusive, after inclusive. */
  before?: string;
  after?: string;
}

export interface ParsedSearch {
  /** The words left once the filters are taken out. */
  text: string;
  /** Channel named by `in:`, or null for the current one. */
  channelId: number | null;
  filters: SearchFilters;
}

const FILTER_RE = /^(from|in|has|before|after):(.*)$/i;
const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/;

/** Local midnight of a YYYY-MM-DD date, `days` later; null if no such day. */
function localDayStart(value: string, days = 0): Date | null {
  const match = DATE_RE.exec(value);
  if (!match) return null;
  const [year, month, day] = match.slice(1).map(Number);
  const date = new Date(year, month - 1, day);
  // `new Date` rolls 2026-02-31 over into March instead of refusing it.
  if (date.getMonth() !== month - 1 || date.getDate() !== day) return null;
  date.setDate(date.getDate() + days);
  return date;
}

/**
 * Split the search box into words and Discord-style filters: `from:alice`,
 * `in:#general`, `has:file`, `before:2026-01-31`, `after:2026-01-01`.
 *
 * `from:` takes the account name, because that is what the server stores on
 * a message and display names are not unique. Dates are whole days in the
 * user's own time zone, and both bounds exclude the named day, as Discord's
 * do. A filter that cannot be honoured is an error rather than dropped:
 * silently searching without it answers a different question.
 */
export function parseSearchQuery(
  raw: string,
  channels: ReadonlyArray<{ id: number; name: string }>
): ParsedSearch | { error: string } {
  const words: string[] = [];
  const filters: SearchFilters = {};
  let channelId: number | null = null;

  for (const token of raw.trim().split(/\s+/)) {
    const match = FILTER_RE.exec(token);
    if (!match) {
      if (token) words.push(token);
      continue;
    }
    const key = match[1].toLowerCase();
    const value = match[2];
    if (!value) return { error: `${key}: needs a value.` };
    switch (key) {
      case 'from':
        filters.from = value.replace(/^@/, '');
        break;
      case 'in': {
        const name = value.replace(/^#/, '').toLowerCase();
        const channel = channels.find((c) => c.name.toLowerCase() === name);
        if (!channel) return { error: `No channel named #${name}.` };
        channelId = channel.id;
        break;
      }
      case 'has':
        if (value.toLowerCase() !== 'file') return { error: 'Only has:file is supported.' };
        filters.hasFile = true;
        break;
      case 'before':
      case 'after': {
        const day = localDayStart(value, key === 'after' ? 1 : 0);
        if (!day) return { error: `${key}: takes a date like 2026-01-31.` };
        filters[key] = day.toISOString();
        break;
      }
    }
  }
  return { text: words.join(' '), channelId, filters };
}
