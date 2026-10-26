import { afterEach, describe, expect, it, vi } from 'vite-plus/test';

import { applyTheme, resolvedLocale } from '../../src/main/preferences';

const root = { dataset: {} as Record<string, string>, style: { colorScheme: '' }, lang: '' };

vi.stubGlobal('document', { documentElement: root });
vi.stubGlobal('navigator', { languages: ['zh-CN'] });

afterEach(() => {
  root.dataset = {};
  root.style.colorScheme = '';
  root.lang = '';
});

describe('main window preferences', () => {
  it('resolves the system language', () => {
    expect(resolvedLocale('system')).toBe('zh');
  });

  it('applies a color theme without using web storage', () => {
    applyTheme('dark');
    expect(root.dataset.theme).toBe('dark');
    expect(root.style.colorScheme).toBe('dark');
  });

  it('resolves explicit Chinese', () => {
    expect(resolvedLocale('zh-CN')).toBe('zh');
  });
});
