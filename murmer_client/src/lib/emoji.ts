/**
 * Inline `:shortcode:` emoji support for message text.
 *
 * Common codes (`:+1:`, `:fire:`, …) resolve to unicode characters; custom
 * server emojis resolve to inline `<img>` tags. Replacement runs on the
 * *sanitized* markdown HTML by walking its text nodes, so the DOMPurify
 * allow-list never has to admit `<img>` (user-authored markdown/HTML still
 * cannot embed arbitrary images) and code spans/blocks keep their literal
 * text. Unknown codes are left untouched.
 */

import type { CustomEmoji } from './stores/customEmojis';

/** Common shortcode → unicode emoji, GitHub/Slack-style names. */
export const EMOJI_CODES: Record<string, string> = {
  '+1': '👍',
  thumbsup: '👍',
  '-1': '👎',
  thumbsdown: '👎',
  smile: '😄',
  smiley: '😃',
  grin: '😁',
  joy: '😂',
  rofl: '🤣',
  laughing: '😆',
  sweat_smile: '😅',
  blush: '😊',
  slight_smile: '🙂',
  upside_down: '🙃',
  wink: '😉',
  heart_eyes: '😍',
  kissing_heart: '😘',
  stuck_out_tongue: '😛',
  thinking: '🤔',
  neutral_face: '😐',
  unamused: '😒',
  roll_eyes: '🙄',
  smirk: '😏',
  grimacing: '😬',
  relieved: '😌',
  sob: '😭',
  cry: '😢',
  angry: '😠',
  rage: '😡',
  scream: '😱',
  fearful: '😨',
  sleeping: '😴',
  sunglasses: '😎',
  nerd_face: '🤓',
  confused: '😕',
  worried: '😟',
  zipper_mouth: '🤐',
  salute: '🫡',
  shrug: '🤷',
  facepalm: '🤦',
  wave: '👋',
  ok_hand: '👌',
  clap: '👏',
  pray: '🙏',
  muscle: '💪',
  raised_hands: '🙌',
  handshake: '🤝',
  v: '✌️',
  crossed_fingers: '🤞',
  point_up: '☝️',
  eyes: '👀',
  brain: '🧠',
  skull: '💀',
  ghost: '👻',
  robot: '🤖',
  alien: '👽',
  poop: '💩',
  heart: '❤️',
  broken_heart: '💔',
  sparkling_heart: '💖',
  purple_heart: '💜',
  blue_heart: '💙',
  green_heart: '💚',
  yellow_heart: '💛',
  black_heart: '🖤',
  fire: '🔥',
  tada: '🎉',
  confetti_ball: '🎊',
  rocket: '🚀',
  star: '⭐',
  sparkles: '✨',
  zap: '⚡',
  boom: '💥',
  '100': '💯',
  check: '✅',
  white_check_mark: '✅',
  heavy_check_mark: '✔️',
  x: '❌',
  warning: '⚠️',
  question: '❓',
  exclamation: '❗',
  bulb: '💡',
  zzz: '💤',
  coffee: '☕',
  beer: '🍺',
  pizza: '🍕',
  cake: '🎂',
  birthday: '🎂',
  gift: '🎁',
  trophy: '🏆',
  crown: '👑',
  gem: '💎',
  moneybag: '💰',
  bug: '🐛',
  dog: '🐶',
  cat: '🐱',
  rainbow: '🌈',
  sunny: '☀️',
  cloud: '☁️',
  snowflake: '❄️',
  speech_balloon: '💬',
  thought_balloon: '💭',
  bell: '🔔',
  lock: '🔒',
  key: '🔑',
  wrench: '🔧',
  hammer: '🔨',
  gear: '⚙️',
  memo: '📝',
  book: '📖',
  calendar: '📅',
  chart_up: '📈',
  chart_down: '📉'
};

/* Custom emoji names are [a-z0-9_]; unicode codes additionally use + and -.
   Not anchored: scans through running text. */
const SHORTCODE_RE = /:([a-z0-9_+-]{1,32}):/g;

/** Resolve `:code:` to a unicode emoji, or null when unknown. */
export function unicodeFromShortcode(value: string): string | null {
  const match = /^:([a-z0-9_+-]{1,32}):$/.exec(value);
  return match ? (EMOJI_CODES[match[1]] ?? null) : null;
}

/* Unicode codepoints that make up emoji: pictographs plus the joiners,
   variation/keycap selectors, regional-indicator pairs and skin-tone
   modifiers that combine into a single glyph. */
const EMOJI_STRIP_RE =
  /[\p{Extended_Pictographic}\u200D\uFE0F\u20E3]|[\u{1F1E6}-\u{1F1FF}]|[\u{1F3FB}-\u{1F3FF}]/gu;

/**
 * True when `text` is made up solely of emoji (unicode characters, known
 * `:shortcode:`s or custom server emoji) and whitespace — used to render
 * emoji-only messages at a larger size. Messages that mix emoji with any
 * other text return false.
 */
export function isEmojiOnlyText(
  text: string,
  emojis: Record<string, CustomEmoji>
): boolean {
  const trimmed = text.trim();
  if (!trimmed) return false;

  let found = false;
  // Drop known shortcodes (unicode and custom); leave unknown ones as text.
  let stripped = trimmed.replace(SHORTCODE_RE, (whole, code: string) => {
    if (EMOJI_CODES[code] || emojis[code]) {
      found = true;
      return '';
    }
    return whole;
  });
  // Drop literal unicode emoji characters.
  stripped = stripped.replace(EMOJI_STRIP_RE, () => {
    found = true;
    return '';
  });

  return found && stripped.trim().length === 0;
}

/* Memoised per (custom emoji map, base) pair: whenever either changes the
   rendered HTML may change, so the cache resets. Same motivation as the
   renderMarkdown cache — message bodies re-evaluate on every list update. */
const MAX_CACHE_ENTRIES = 500;
let cache = new Map<string, string>();
let cacheEmojis: Record<string, CustomEmoji> | null = null;
let cacheBase = '';

/**
 * Replace known `:shortcode:` occurrences in sanitized message HTML with
 * unicode emoji or inline `<img>` tags for custom server emojis.
 */
export function emojifyHtml(
  html: string,
  emojis: Record<string, CustomEmoji>,
  base: string
): string {
  if (!html.includes(':') || typeof document === 'undefined') return html;

  if (emojis !== cacheEmojis || base !== cacheBase) {
    cache = new Map();
    cacheEmojis = emojis;
    cacheBase = base;
  }
  const cached = cache.get(html);
  if (cached !== undefined) {
    cache.delete(html);
    cache.set(html, cached);
    return cached;
  }

  const template = document.createElement('template');
  template.innerHTML = html;

  const walker = document.createTreeWalker(template.content, NodeFilter.SHOW_TEXT);
  const textNodes: Text[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!node.nodeValue?.includes(':')) continue;
    // Code spans and blocks keep their literal text.
    let ancestor = node.parentElement;
    let inCode = false;
    while (ancestor) {
      if (ancestor.tagName === 'CODE' || ancestor.tagName === 'PRE') {
        inCode = true;
        break;
      }
      ancestor = ancestor.parentElement;
    }
    if (!inCode) textNodes.push(node as Text);
  }

  let changed = false;
  for (const node of textNodes) {
    const text = node.nodeValue ?? '';
    SHORTCODE_RE.lastIndex = 0;
    let fragment: DocumentFragment | null = null;
    let consumed = 0;
    let match: RegExpExecArray | null;
    while ((match = SHORTCODE_RE.exec(text))) {
      const code = match[1];
      const unicode = EMOJI_CODES[code];
      const custom = unicode ? undefined : emojis[code];
      if (!unicode && !custom) {
        // The closing colon may open the next code (e.g. ":unknown:fire:"),
        // so resume the scan on it instead of after it.
        SHORTCODE_RE.lastIndex = match.index + match[0].length - 1;
        continue;
      }
      fragment ??= document.createDocumentFragment();
      if (match.index > consumed) {
        fragment.appendChild(document.createTextNode(text.slice(consumed, match.index)));
      }
      if (unicode) {
        fragment.appendChild(document.createTextNode(unicode));
      } else if (custom) {
        const img = document.createElement('img');
        img.className = 'inline-emoji';
        img.src = base + custom.url;
        img.alt = match[0];
        img.title = match[0];
        img.loading = 'lazy';
        img.draggable = false;
        fragment.appendChild(img);
      }
      consumed = match.index + match[0].length;
    }
    if (fragment) {
      if (consumed < text.length) {
        fragment.appendChild(document.createTextNode(text.slice(consumed)));
      }
      node.replaceWith(fragment);
      changed = true;
    }
  }

  const result = changed ? template.innerHTML : html;
  if (cache.size >= MAX_CACHE_ENTRIES) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(html, result);
  return result;
}
