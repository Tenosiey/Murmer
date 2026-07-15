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

marked.use({ renderer, extensions: [wikilinkExtension as any] });

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

  // Use parseInline for simple text to avoid wrapping in <p> tags
  // Only use full parse if the text contains markdown syntax
  const hasMarkdown = /[*_`~\[\]#>|\\]/.test(text) || text.includes('\n\n');

  const html = hasMarkdown
    ? marked.parse(text) as string
    : marked.parseInline(text) as string;

  // Keep the wikilink target attributes; DOMPurify strips unknown
  // data-* attributes by default.
  const sanitized = DOMPurify.sanitize(html, {
    ADD_ATTR: ['data-wiki-channel', 'data-wiki-slug']
  });

  if (renderCache.size >= MAX_RENDER_CACHE_ENTRIES) {
    const oldest = renderCache.keys().next().value;
    if (oldest !== undefined) renderCache.delete(oldest);
  }
  renderCache.set(text, sanitized);
  return sanitized;
}
