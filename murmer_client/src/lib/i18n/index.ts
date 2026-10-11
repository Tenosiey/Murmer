/**
 * The UI string lookup. Every user-facing string lives in a catalog keyed by
 * a stable id, `en.ts` being the source of truth, and components render
 * `t('login.title')` instead of the English inline.
 *
 * Only English ships today. Adding a language is a new catalog file typed
 * `Catalog` plus one line in `CATALOGS`; a key the new catalog leaves out
 * falls back to English rather than rendering the bare id, so a half-done
 * translation degrades to mixed language instead of to gibberish.
 *
 * Messages take `{name}` placeholders, never string concatenation, because
 * word order differs between languages. A message that varies with a number
 * is an object of CLDR plural forms (`one`, `few`, `other`, ...) picked by
 * `Intl.PluralRules`, since `count === 1 ? '' : 's'` is English grammar and
 * wrong for most other languages.
 */
import { en } from './en';

export type PluralMessage = Partial<Record<Intl.LDMLPluralRule, string>> & { other: string };
export type Message = string | PluralMessage;
export type MessageKey = keyof typeof en;
export type Catalog = Partial<Record<MessageKey, Message>>;
export type MessageParams = Record<string, string | number>;

const CATALOGS: Record<string, Catalog> = { en };

/**
 * The locale is picked once, at startup, from the browser's language.
 * ponytail: not reactive; a language picker would set the locale and reload
 * the page rather than make every `t()` call a store subscription.
 */
function detectLocale(): string {
  const preferred = typeof navigator === 'undefined' ? [] : navigator.languages ?? [];
  for (const tag of preferred) {
    const base = tag.split('-')[0];
    if (Object.hasOwn(CATALOGS, base)) return base;
  }
  return 'en';
}

const locale = detectLocale();
const catalog = CATALOGS[locale];
const plurals = new Intl.PluralRules(locale);

/** Whether `key` is in the catalog; for keys built from untrusted input. */
export function hasMessage(key: string): key is MessageKey {
  return Object.hasOwn(en, key);
}

/** Look up `key` and fill its `{placeholders}` from `params`. */
export function t(key: MessageKey, params: MessageParams = {}): string {
  const message: Message = catalog[key] ?? en[key];
  let text: string;
  if (typeof message === 'string') {
    text = message;
  } else {
    const count = Number(params.count);
    text = message[plurals.select(count)] ?? message.other;
  }
  // An unknown placeholder stays visible as `{name}` so the bug shows in the
  // UI rather than silently rendering an empty gap.
  return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    Object.hasOwn(params, name) ? String(params[name]) : whole
  );
}
