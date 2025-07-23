import { marked } from 'marked';
import DOMPurify from 'dompurify';

export function renderMarkdown(text: string): string {
  // Use parseInline for simple text to avoid wrapping in <p> tags
  // Only use full parse if the text contains markdown syntax
  const hasMarkdown = /[*_`~\[\]#>|\\]/.test(text) || text.includes('\n\n');
  
  const html = hasMarkdown 
    ? marked.parse(text) as string
    : marked.parseInline(text) as string;
    
  return DOMPurify.sanitize(html);
}
