import { marked } from 'marked';
import type { Tokens } from 'marked';
import DOMPurify from 'dompurify';
import hljs from 'highlight.js/lib/common';
import { parseWikiTarget } from './wiki/slug';

const renderer = new marked.Renderer();
const defaultCodeRenderer = renderer.code.bind(renderer);

renderer.code = ({ text, lang, escaped }: Tokens.Code) => {
  const normalized = (lang ?? '').trim().split(/\s+/)[0]?.toLowerCase() ?? '';

  const buildBlock = (value: string, language: string | undefined) => {
    const languageClass = language ? ` language-${language}` : normalized ? ` language-${normalized}` : '';
    return `<pre><code class="hljs${languageClass}">${value}</code></pre>`;
  };

  if (normalized && hljs.getLanguage(normalized)) {
    try {
      const result = hljs.highlight(text, { language: normalized, ignoreIllegals: true });
      return buildBlock(result.value, result.language ?? normalized);
    } catch (error) {
      console.warn('Failed to highlight code block', error);
    }
  }

  try {
    const result = hljs.highlightAuto(text);
    if (result?.value) {
      return buildBlock(result.value, result.language);
    }
  } catch (error) {
    console.warn('Failed to auto-highlight code block', error);
  }

  return defaultCodeRenderer({
    type: 'code',
    raw: text,
    text,
    lang,
    escaped
  });
};

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * Inline extension for `[[page]]` / `[[channel/page]]` / `[[target|label]]`
 * wiki links. Emits anchors carrying the target in data attributes; click
 * handling and missing-page styling are applied by the `wikilinks` action
 * (`src/lib/wiki/links.ts`) after rendering, so the render cache never holds
 * stale existence state. Runs at the inline level, so code spans and fenced
 * blocks are unaffected.
 */
const wikilinkExtension = {
  name: 'wikilink',
  level: 'inline' as const,
  start(src: string) {
    const index = src.indexOf('[[');
    return index === -1 ? undefined : index;
  },
  tokenizer(src: string) {
    const match = /^\[\[([^[\]|]+?)(?:\|([^[\]]+?))?\]\]/.exec(src);
    if (!match) return undefined;
    const target = parseWikiTarget(match[1], match[2]);
    if (!target) return undefined;
    return {
      type: 'wikilink',
      raw: match[0],
      channel: target.channel ?? '',
      slug: target.slug,
      label: target.label
    };
  },
  renderer(token: { channel: string; slug: string; label: string }) {
    const channel = escapeHtml(token.channel);
    const slug = escapeHtml(token.slug);
    return `<a class="wikilink" href="#" data-wiki-channel="${channel}" data-wiki-slug="${slug}">${escapeHtml(token.label)}</a>`;
  }
};

/**
 * Inline extension for `||spoiler||`. The content is ordinary inline
 * markdown; `spoilers.ts` reveals it on click or Enter. Needs a non-space
 * right inside both bars, so a `||` used as "or" in prose stays text.
 */
const spoilerExtension = {
  name: 'spoiler',
  level: 'inline' as const,
  start(src: string) {
    const index = src.indexOf('||');
    return index === -1 ? undefined : index;
  },
  tokenizer(this: { lexer: { inlineTokens(src: string): Tokens.Generic[] } }, src: string) {
    const match = /^\|\|(?=\S)([\s\S]*?\S)\|\|/.exec(src);
    if (!match) return undefined;
    return {
      type: 'spoiler',
      raw: match[0],
      tokens: this.lexer.inlineTokens(match[1])
    };
  },
  renderer(
    this: { parser: { parseInline(tokens: Tokens.Generic[]): string } },
    token: { tokens: Tokens.Generic[] }
  ) {
    return `<span class="spoiler" role="button" tabindex="0" aria-label="Spoiler, select to reveal">${this.parser.parseInline(token.tokens)}</span>`;
  }
};

/* An image in a message is fetched by every reader's client the moment it
   renders, so its author learns each reader's IP address, and when they read
   it. Markdown images therefore render as a plain link to the image; pictures
   meant for the chat go through `/upload`, whose files live on the server. */
renderer.image = ({ href, text }: Tokens.Image) =>
  `<a href="${escapeHtml(href)}">${escapeHtml(text || href)}</a>`;

marked.use({ renderer, extensions: [wikilinkExtension as any, spoilerExtension as any] });

/* A link in a message is somebody else's URL. Followed in place it replaces
   the app itself — in the desktop shell the page then sits inside the Murmer
   window, where a copy of the login or backup screen is a convincing way to
   ask for a recovery phrase. So every external link opens outside the app
   (the opener plugin hands `_blank` to the system browser), and without an
   opener or referrer. Wikilinks (`href="#"`) are left to their click handler. */
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A' && /^(https?|mailto):/i.test(node.getAttribute('href') ?? '')) {
    node.setAttribute('target', '_blank');
    node.setAttribute('rel', 'noopener noreferrer');
  }
});

/** Characters that can only appear as markdown syntax often enough to be worth
 *  a full parse: emphasis, code, strikethrough, links, headings, quotes,
 *  tables, escapes. */
const INLINE_MARKERS = /[*_`~\[\]#>|\\]/;

/** List items, whose markers are absent from `INLINE_MARKERS` — `- x`, `+ x`,
 *  `1. x`, `1) x`. A marker must start its line and be followed by a space, so
 *  "well-known", "-5 Grad" and "12.5 Prozent" stay plain text.
 *
 *  Setext underlines (`===`, `---`) are deliberately *not* detected: they turn
 *  the line above them into a heading, so a message using a row of dashes as a
 *  separator would silently swallow its own first line. `#` headings cover the
 *  intentional case. */
const LIST_MARKER = /^[ \t]*(?:[-+][ \t]|\d{1,9}[.)][ \t])/m;

/* Rendering is memoised because the chat view re-evaluates message bodies
   whenever the message list updates; parsing + sanitising + highlighting the
   same text repeatedly is by far the most expensive part of a chat update. */
const MAX_RENDER_CACHE_ENTRIES = 500;
const renderCache = new Map<string, string>();

export function renderMarkdown(text: string): string {
  const cached = renderCache.get(text);
  if (cached !== undefined) {
    // Refresh recency so frequently visible messages stay cached.
    renderCache.delete(text);
    renderCache.set(text, cached);
    return cached;
  }

  // Use parseInline for simple text to avoid wrapping in <p> tags; only use
  // the full parse when the text actually contains markdown syntax. The
  // detector may over-report (a needless full parse just costs time) but must
  // never under-report — a missed construct renders as literal text.
  const hasMarkdown = INLINE_MARKERS.test(text) || LIST_MARKER.test(text) || text.includes('\n\n');

  const html = hasMarkdown
    ? marked.parse(text) as string
    : marked.parseInline(text) as string;

  /* `ALLOW_DATA_ATTR` defaults to true, which would let a message author put
     any `data-*` on any element. Nothing in this pipeline emits data
     attributes except the wikilink extension, and `wiki/links.ts` reads those
     two back off the rendered DOM — so allow exactly them and deny the rest.
     That keeps a message from ever forging state that app code reads out of
     rendered content. */
  const sanitized = DOMPurify.sanitize(html, {
    ALLOW_DATA_ATTR: false,
    ADD_ATTR: ['data-wiki-channel', 'data-wiki-slug'],
    /* Nothing in a message may make the reader's client fetch a URL on its
       own: that turns any member into a tracker of who read what, from where
       (see `renderer.image`). Raw HTML is the other way in, so the elements
       and attributes that load a resource are dropped — SVG and MathML with
       them, which markdown never emits. */
    USE_PROFILES: { html: true },
    FORBID_TAGS: ['img', 'audio', 'video', 'source', 'track', 'picture', 'input'],
    FORBID_ATTR: ['style', 'src', 'srcset', 'poster', 'background']
  });

  if (renderCache.size >= MAX_RENDER_CACHE_ENTRIES) {
    const oldest = renderCache.keys().next().value;
    if (oldest !== undefined) renderCache.delete(oldest);
  }
  renderCache.set(text, sanitized);
  return sanitized;
}
