import { marked } from 'marked';
import type { Tokens } from 'marked';
import DOMPurify from 'dompurify';
import hljs from 'highlight.js/lib/common';

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

marked.use({ renderer });

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

  const sanitized = DOMPurify.sanitize(html);

  if (renderCache.size >= MAX_RENDER_CACHE_ENTRIES) {
    const oldest = renderCache.keys().next().value;
    if (oldest !== undefined) renderCache.delete(oldest);
  }
  renderCache.set(text, sanitized);
  return sanitized;
}
