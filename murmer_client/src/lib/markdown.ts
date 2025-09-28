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

export function renderMarkdown(text: string): string {
  // Use parseInline for simple text to avoid wrapping in <p> tags
  // Only use full parse if the text contains markdown syntax
  const hasMarkdown = /[*_`~\[\]#>|\\]/.test(text) || text.includes('\n\n');
  
  const html = hasMarkdown 
    ? marked.parse(text) as string
    : marked.parseInline(text) as string;
    
  return DOMPurify.sanitize(html);
}
