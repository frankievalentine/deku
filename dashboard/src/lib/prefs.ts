import { useSyncExternalStore } from 'react';

export type ThemeMode = 'system' | 'light' | 'dark';

export interface Prefs {
  theme: ThemeMode;
}

const STORAGE_KEY = 'deku-next-prefs';
const listeners = new Set<() => void>();

function read(): Prefs {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}') as Partial<Prefs>;
    return {
      theme: parsed.theme === 'light' || parsed.theme === 'dark' ? parsed.theme : 'system',
    };
  } catch {
    return { theme: 'system' };
  }
}

let current = read();
const media = window.matchMedia('(prefers-color-scheme: dark)');

function apply(prefs: Prefs): void {
  const root = document.documentElement;
  const dark = prefs.theme === 'dark' || (prefs.theme === 'system' && media.matches);
  root.dataset.theme = dark ? 'dark' : 'light';
}

media.addEventListener('change', () => apply(current));
apply(current);

export function setPrefs(next: Partial<Prefs>): void {
  current = { ...current, ...next };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(current));
  } catch {
    // Storage can be unavailable (private windows); the choice still applies for this visit.
  }
  apply(current);
  for (const listener of listeners) listener();
}

export function usePrefs(): Prefs {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current
  );
}
