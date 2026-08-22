// @vitest-environment jsdom

/**
 * `renderMarkdown` output goes straight into `{@html}`, so DOMPurify is the
 * only thing between a chat message and script execution — these tests are
 * that boundary's regression net.
 *
 * This is the one suite that opts out of the Node default in
 * `vitest.config.ts`: DOMPurify needs a real DOM to parse into. **jsdom, not
 * happy-dom** — happy-dom 20 mis-drives DOMPurify's tree walk badly enough
 * that `DOMPurify.sanitize('before <script>alert(1)</script> after')` returns
 * the script tag intact and drops the first element of the body instead.
 * Tests written against it would have asserted the opposite of the truth,
 * which on a security boundary is worse than no tests at all.
 */

import { describe, expect, it, vi } from 'vitest';
import DOMPurify from 'dompurify';
import { renderMarkdown } from './markdown';

/** Render and parse, so assertions can talk about nodes instead of substrings. */
function render(text: string): HTMLElement {
  const host = document.createElement('div');
  host.innerHTML = renderMarkdown(text);
  return host;
}

/** Every attribute name in the rendered tree — used to prove no `on*` survives. */
function attributeNames(host: HTMLElement): string[] {
  return [...host.querySelectorAll('*')].flatMap((el) => [...el.attributes].map((a) => a.name));
}

describe('renderMarkdown / rendering', () => {
  it('leaves plain text unwrapped', () => {
    // The `hasMarkdown` heuristic sends syntax-free text through
    // `parseInline`, so a one-line chat message renders without a <p>.
    expect(renderMarkdown('hello world')).toBe('hello world');
  });

  it('renders block markdown once syntax is present', () => {
    expect(renderMarkdown('**bold** and _em_')).toBe('<p><strong>bold</strong> and <em>em</em></p>\n');
    expect(renderMarkdown('~~strike~~')).toBe('<p><del>strike</del></p>\n');
    expect(renderMarkdown('# Heading')).toBe('<h1>Heading</h1>\n');
    expect(renderMarkdown('> quote')).toBe('<blockquote>\n<p>quote</p>\n</blockquote>\n');
  });

  it('keeps already-escaped markup escaped', () => {
    const text = '&lt;script&gt;alert(1)&lt;/script&gt;';
    expect(renderMarkdown(text)).toBe(text);
    expect(render(text).querySelector('script')).toBeNull();
  });

  it('keeps ordinary links intact', () => {
    const host = render('[link](https://example.com)');
    expect(host.querySelector('a')?.getAttribute('href')).toBe('https://example.com');
  });

  it('renders nothing for empty input', () => {
    expect(renderMarkdown('')).toBe('');
  });
});

describe('renderMarkdown / sanitisation', () => {
  it('removes script elements, alone and inline', () => {
    for (const text of [
      '<script>alert(1)</script>',
      'before <script>alert(1)</script> after',
      '<p>a</p><script src="https://evil.test/x.js"></script>',
      '<svg><script>alert(1)</script></svg>'
    ]) {
      const host = render(text);
      expect(host.querySelector('script'), text).toBeNull();
      // KEEP_CONTENT must not smuggle the body back out as text either.
      expect(host.textContent, text).not.toContain('alert(1)');
    }
  });

  it('strips event-handler attributes while keeping the element', () => {
    const host = render('<img src=x onerror="alert(1)">');
    expect(host.querySelector('img')?.getAttribute('src')).toBe('x');
    expect(attributeNames(host)).toEqual(['src']);
  });

  it('strips event handlers from every element it keeps', () => {
    for (const text of [
      '<div onclick="alert(1)">x</div>',
      '<audio src=x onloadstart=alert(1)>',
      '<a href="#" data-wiki-slug="ok" onclick="alert(1)">x</a>'
    ]) {
      expect(attributeNames(render(text)).filter((name) => name.startsWith('on')), text).toEqual([]);
    }
  });

  it('drops elements that load or execute foreign content', () => {
    for (const [text, selector] of [
      ['<iframe src="https://evil.test"></iframe>', 'iframe'],
      ['<object data="x"></object>', 'object'],
      ['<embed src="x">', 'embed'],
      ['<link rel=stylesheet href="https://evil.test/x.css">', 'link'],
      ['<meta http-equiv="refresh" content="0;url=https://evil.test">', 'meta'],
      ['<base href="https://evil.test/">', 'base'],
      ['<style>body{display:none}</style>', 'style']
    ] as const) {
      expect(render(text).querySelector(selector), text).toBeNull();
    }
  });

  it('drops script-bearing URL schemes but keeps http(s)', () => {
    for (const text of [
      '[click](javascript:alert(1))',
      '[click](JaVaScRiPt:alert(1))',
      '<a href="javascript:alert(1)">x</a>',
      '<a href=" javascript:alert(1)">x</a>',
      '<a href="java\tscript:alert(1)">x</a>',
      '[x](vbscript:alert(1))',
      '[x](data:text/html,<script>alert(1)</script>)'
    ]) {
      const anchor = render(text).querySelector('a');
      expect(anchor, text).not.toBeNull();
      expect(anchor?.getAttribute('href'), text).toBeNull();
    }

    expect(render('<a href="https://example.com">x</a>').querySelector('a')?.getAttribute('href')).toBe(
      'https://example.com'
    );
  });

  it('neutralises the noscript namespace-confusion mXSS vector', () => {
    // `<p title="</noscript>…">` re-parses as markup in browsers that render
    // noscript content; DOMPurify's mXSS pass must kill it before {@html} does.
    const host = render('<noscript><p title="</noscript><img src=x onerror=alert(1)>">');
    expect(host.querySelector('img')).toBeNull();
    expect(attributeNames(host)).toEqual([]);
  });

  it('cannot be escaped through a markdown link title', () => {
    const host = render('[x](https://example.com "title\\" onmouseover=\\"alert(1)")');
    expect(attributeNames(host).sort()).toEqual(['href', 'title']);
  });
});

describe('renderMarkdown / code blocks', () => {
  it('highlights fenced code and escapes the HTML inside it', () => {
    const html = renderMarkdown('```js\nconst a = "<b>";\n```');
    expect(html.startsWith('<pre><code class="hljs language-js">')).toBe(true);

    const host = render('```js\nconst a = "<b>";\n```');
    expect(host.querySelector('pre b')).toBeNull();
    expect(host.querySelector('pre')?.textContent).toBe('const a = "<b>";');
  });

  it('escapes a script tag written inside a fence', () => {
    const host = render('```\n<script>alert(1)</script>\n```');
    expect(host.querySelector('script')).toBeNull();
    expect(host.querySelector('pre')?.textContent).toBe('<script>alert(1)</script>');
  });

  it('falls back to auto-detection for an unknown language', () => {
    expect(renderMarkdown('```notalanguage\nplain text\n```').startsWith('<pre><code class="hljs')).toBe(true);
  });

  it('escapes HTML in inline code spans', () => {
    const host = render('`<b>inline</b>`');
    expect(host.querySelector('code b')).toBeNull();
    expect(host.querySelector('code')?.textContent).toBe('<b>inline</b>');
  });
});

describe('renderMarkdown / wiki links', () => {
  it('keeps the target data attributes through sanitisation', () => {
    // The `wikilinks` action reads these back; a sanitiser config that dropped
    // them would leave every [[link]] silently dead.
    const host = render('[[docs/Setup|The setup]]');
    const anchor = host.querySelector('a.wikilink');
    expect(anchor?.getAttribute('data-wiki-channel')).toBe('docs');
    expect(anchor?.getAttribute('data-wiki-slug')).toBe('setup');
    expect(anchor?.textContent).toBe('The setup');
  });

  it('slugifies the page part and defaults the label to it', () => {
    const anchor = render('[[Getting Started]]').querySelector('a.wikilink');
    expect(anchor?.getAttribute('data-wiki-channel')).toBe('');
    expect(anchor?.getAttribute('data-wiki-slug')).toBe('getting-started');
    expect(anchor?.textContent).toBe('Getting Started');
  });

  it('cannot break out of the generated attributes or add new ones', () => {
    for (const text of ['[[" onmouseover="alert(1)]]', '[[a"><img src=x onerror=alert(1)>]]']) {
      const host = render(text);
      expect(host.querySelector('img'), text).toBeNull();
      expect(attributeNames(host).sort(), text).toEqual([
        'class',
        'data-wiki-channel',
        'data-wiki-slug',
        'href'
      ]);
    }
  });

  it('leaves targets that produce no slug as literal text', () => {
    for (const text of ['[[]]', '[[   ]]', '[[///]]', '[[chan/]]']) {
      const host = render(text);
      expect(host.querySelector('a'), text).toBeNull();
      expect(host.textContent?.trim(), text).toBe(text);
    }
  });
});

describe('renderMarkdown / render cache', () => {
  /** Mirrors `MAX_RENDER_CACHE_ENTRIES` in markdown.ts. */
  const MAX_RENDER_CACHE_ENTRIES = 500;

  /** Unique per call so tests never collide in the module-level cache. */
  let counter = 0;
  const unique = (suffix = '') => `cache probe ${counter++} ${suffix}`;

  it('sanitises once and serves the sanitised copy afterwards', () => {
    const text = `${unique()} <img src=x onerror="alert(1)">`;
    const first = renderMarkdown(text);

    const sanitize = vi.spyOn(DOMPurify, 'sanitize');
    try {
      const second = renderMarkdown(text);
      expect(sanitize).not.toHaveBeenCalled();
      // A cache that handed back the pre-sanitised HTML would be the whole
      // boundary undone, so assert on the value, not just on the cache hit.
      expect(second).toBe(first);
      expect(second).not.toContain('onerror');
    } finally {
      sanitize.mockRestore();
    }
  });

  it('evicts the least recently used entry once it is full', () => {
    const text = unique('evicted');
    renderMarkdown(text);
    for (let i = 0; i < MAX_RENDER_CACHE_ENTRIES; i += 1) renderMarkdown(unique('filler'));

    const sanitize = vi.spyOn(DOMPurify, 'sanitize');
    try {
      expect(renderMarkdown(text)).toBe(text);
      expect(sanitize).toHaveBeenCalledTimes(1);
    } finally {
      sanitize.mockRestore();
    }
  });

  it('keeps an entry alive while it is still being used', () => {
    // The delete/set dance on a hit is what makes a visible message survive a
    // burst of new ones; without it this entry would have been evicted.
    const text = unique('refreshed');
    renderMarkdown(text);
    for (let i = 0; i < MAX_RENDER_CACHE_ENTRIES - 1; i += 1) renderMarkdown(unique('filler'));
    renderMarkdown(text);
    for (let i = 0; i < MAX_RENDER_CACHE_ENTRIES / 2; i += 1) renderMarkdown(unique('filler'));

    const sanitize = vi.spyOn(DOMPurify, 'sanitize');
    try {
      expect(renderMarkdown(text)).toBe(text);
      expect(sanitize).not.toHaveBeenCalled();
    } finally {
      sanitize.mockRestore();
    }
  });
});
