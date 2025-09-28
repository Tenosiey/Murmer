import { writable, get } from 'svelte/store';

export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'murmer-theme';

function applyTheme(value: Theme) {
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.theme = value;
  }
}

function createThemeStore() {
  const store = writable<Theme>('dark');

  return {
    subscribe: store.subscribe,
    set: (value: Theme) => {
      store.set(value);
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(STORAGE_KEY, value);
      }
      applyTheme(value);
    },
    toggle: () => {
      const nextValue: Theme = get(store) === 'dark' ? 'light' : 'dark';
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(STORAGE_KEY, nextValue);
      }
      store.set(nextValue);
      applyTheme(nextValue);
    },
    init: () => {
      if (typeof window === 'undefined') return;
      const stored = localStorage.getItem(STORAGE_KEY) as Theme | null;
      const initial: Theme = stored ?? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
      store.set(initial);
      applyTheme(initial);
    }
  };
}

export const theme = createThemeStore();
