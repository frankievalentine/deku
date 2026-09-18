/**
 * Basecoat 1.0 ships untyped IIFE bundles under `basecoat-css/<component>`.
 * Each one registers itself on `window.basecoat`, so the runtime must be
 * imported and awaited before any component module is imported.
 */

declare module 'basecoat-css/basecoat';
declare module 'basecoat-css/select';
declare module 'basecoat-css/popover';
declare module 'basecoat-css/sidebar';
declare module 'basecoat-css/tabs';
declare module 'basecoat-css/command';
declare module 'basecoat-css/toast';

interface BasecoatComponentOptions {
  selector: string;
  init: (element: HTMLElement) => void;
  refresh?: (element: HTMLElement) => void;
}

interface BasecoatRuntime {
  register: (
    name: string,
    selectorOrOptions: string | BasecoatComponentOptions,
    init?: (element: HTMLElement) => void
  ) => void;
  init: (name: string, options?: { force?: boolean }) => void;
  initAll: (options?: { force?: boolean }) => void;
  refresh: (element: HTMLElement) => void;
  start: () => void;
  stop: () => void;
  theme: {
    get: () => 'light' | 'dark';
    set: (mode: 'light' | 'dark') => void;
    toggle: () => void;
  };
}

interface Window {
  basecoat: BasecoatRuntime;
}
