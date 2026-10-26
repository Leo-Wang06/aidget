import { systemLocale, type Locale } from './locale';

export type Theme = 'system' | 'light' | 'dark';

/** Apply the persisted theme to the document without touching web storage. */
export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme === 'system' ? 'light dark' : theme;
}

/** Resolve an explicit or system-following selection to a translation locale. */
export function resolvedLocale(locale: Locale): 'en' | 'zh' {
  if (locale === 'system') {
    return systemLocale(navigator.languages);
  }
  return locale === 'zh-CN' ? 'zh' : 'en';
}
