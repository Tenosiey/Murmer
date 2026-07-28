import { beforeEach } from 'vitest';

/**
 * Minimal in-memory `Storage`. The stores under test only persist small JSON
 * blobs, so a real DOM implementation (jsdom/happy-dom) would buy nothing but
 * install time. Cleared before every test so persisted state cannot leak
 * between them.
 */
function createMemoryStorage(): Storage {
  const entries = new Map<string, string>();
  return {
    get length() {
      return entries.size;
    },
    key(index: number) {
      return [...entries.keys()][index] ?? null;
    },
    getItem(key: string) {
      return entries.get(key) ?? null;
    },
    setItem(key: string, value: string) {
      entries.set(key, String(value));
    },
    removeItem(key: string) {
      entries.delete(key);
    },
    clear() {
      entries.clear();
    }
  } as Storage;
}

const memoryStorage = createMemoryStorage();

Object.defineProperty(globalThis, 'localStorage', {
  value: memoryStorage,
  configurable: true,
  writable: true
});

beforeEach(() => {
  memoryStorage.clear();
});
