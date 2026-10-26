import { describe, expect, it } from 'vite-plus/test';

const ORIGIN = 'aidget://localhost';
import {
  type WidgetState,
  contentOrigin,
  widgetUrl,
  widgetFramePolicy,
  widgetEventTarget,
  widgetIdFromLabel,
  isOwnWidget,
  sandboxAttribute,
  sandboxTokens,
  shouldReload,
} from '../../src/widget/url';

describe('sandbox', () => {
  it('permits the trusted drag bridge while CSP disables static widget scripts', () => {
    expect(sandboxTokens(false, 'all')).toEqual(['allow-scripts']);
    expect(sandboxAttribute(false, 'all')).toBe('allow-scripts');
    expect(sandboxTokens(false, 'handle')).toEqual([]);
  });

  it('only grants scripts, never same-origin', () => {
    expect(sandboxTokens(true)).toEqual(['allow-scripts']);
    expect(sandboxAttribute(true)).not.toContain('allow-same-origin');
  });
});

describe('widget url', () => {
  it('limits child navigations to this widget entrypoint on either platform origin', () => {
    expect(widgetFramePolicy(ORIGIN, 'note')).toBe(
      'frame-src aidget://localhost/content/note/index.html',
    );
    expect(widgetFramePolicy('http://aidget.localhost', 'note')).toBe(
      'frame-src http://aidget.localhost/content/note/index.html',
    );
  });
  it('encodes the id and revision-token cache buster', () => {
    expect(widgetUrl('aidget://localhost', 'weekly-todo', 7, 'abc')).toBe(
      'aidget://localhost/content/weekly-todo/index.html?v=7-abc',
    );
  });

  it('uses the page origin so Windows gets its http host', () => {
    const windows = contentOrigin({
      origin: 'http://aidget.localhost',
      hostname: 'aidget.localhost',
    });
    expect(widgetUrl(windows, 'a', 1, 't')).toBe(
      'http://aidget.localhost/content/a/index.html?v=1-t',
    );
  });

  it('refuses to guess an origin for a foreign page', () => {
    expect(() =>
      contentOrigin({ origin: 'https://example.com', hostname: 'example.com' }),
    ).toThrow();
  });

  it('scopes widget events to the window that owns them', () => {
    expect(widgetEventTarget('widget-weather')).toEqual({
      kind: 'AnyLabel',
      label: 'widget-weather',
    });
  });

  it('rejects state that belongs to another widget window', () => {
    const own = {
      id: 'welcome',
      revision: 2,
      token: 't',

      dragMode: 'all' as const,
      permissions: [],
      hostSurface: false,
      title: 'welcome',
    };
    expect(isOwnWidget('welcome', own)).toBe(true);
    expect(isOwnWidget('weather', own)).toBe(false);
    expect(isOwnWidget(null, own)).toBe(false);
  });

  it('derives the widget id from the window label', () => {
    expect(widgetIdFromLabel('widget-weather')).toBe('weather');
    expect(widgetIdFromLabel('main')).toBeNull();
  });
});

describe('reload', () => {
  const base = {
    id: 'a',
    revision: 1,
    token: 't0',

    dragMode: 'all' as const,
    permissions: [],
    hostSurface: true,
    title: 'a',
  };

  const url = (state: typeof base) => widgetUrl(ORIGIN, state.id, state.revision, state.token);

  it('reloads on revision change', () => {
    const next = { ...base, revision: 2 };
    expect(shouldReload(base, next, url(base), ORIGIN)).toBe(true);
  });

  it('reloads when only the file changed', () => {
    const next = { ...base, token: 't1' };
    expect(shouldReload(base, next, url(base), ORIGIN)).toBe(true);
  });

  it('reloads when scripts are enabled', () => {
    const next: WidgetState = { ...base, permissions: ['script'] };
    expect(shouldReload(base, next, url(base), ORIGIN)).toBe(true);
  });

  it('reloads when the declared permissions change', () => {
    const next: WidgetState = { ...base, permissions: ['location'] };
    expect(shouldReload(base, next, url(base), ORIGIN)).toBe(true);
  });

  it('reloads when the page moved to another aidget origin', () => {
    expect(shouldReload(base, base, url(base), 'http://aidget.localhost')).toBe(true);
  });

  it('stays put when nothing changed', () => {
    expect(shouldReload(base, base, url(base), ORIGIN)).toBe(false);
  });

  it('reloads when the drag policy changes', () => {
    expect(shouldReload(base, { ...base, dragMode: 'handle' }, url(base), ORIGIN)).toBe(true);
  });
});
