export type ThemeMode = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

export const THEME_STORAGE_KEY = 'deku_theme';
export const THEME_CHANGE_EVENT = 'deku:theme-change';

export function getStoredThemeMode(): ThemeMode {
  if (typeof window === 'undefined') return 'system';
  const value = window.localStorage.getItem(THEME_STORAGE_KEY);
  if (value === 'light' || value === 'dark' || value === 'system') {
    return value;
  }
  return 'system';
}

export function resolveTheme(mode: ThemeMode): ResolvedTheme {
  if (mode === 'light' || mode === 'dark') {
    return mode;
  }

  if (typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches) {
    return 'dark';
  }

  return 'light';
}

export function applyTheme(mode: ThemeMode, persist = true): ResolvedTheme {
  if (typeof document === 'undefined') {
    return mode === 'dark' ? 'dark' : 'light';
  }

  const resolved = resolveTheme(mode);
  const root = document.documentElement;

  root.classList.toggle('dark', resolved === 'dark');
  root.dataset.themeMode = mode;
  root.style.colorScheme = resolved;

  if (persist && typeof window !== 'undefined') {
    window.localStorage.setItem(THEME_STORAGE_KEY, mode);
  }

  document.dispatchEvent(
    new CustomEvent(THEME_CHANGE_EVENT, {
      detail: { mode, resolved },
    })
  );

  return resolved;
}
