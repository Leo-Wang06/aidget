/// @vitest-environment happy-dom

import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vite-plus/test';

const widgetHtml = readFileSync('ui/widget.html', 'utf8');
const shellHtml = widgetHtml
  .replace('    <link rel="stylesheet" href="./src/widget/index.css" />\n', '')
  .replace(
    '      <iframe class="widget" title="aidget widget" sandbox referrerpolicy="no-referrer"></iframe>\n',
    '',
  )
  .replace('    <script type="module" src="./src/widget/index.ts"></script>\n', '');

describe('widget window shell', () => {
  it('hides fallback surfaces and faults before widget state loads', () => {
    const page = new DOMParser().parseFromString(shellHtml, 'text/html');
    const chrome = page.querySelector<HTMLElement>('.widget-chrome');
    const surface = page.querySelector<HTMLElement>('.host-surface');
    const fault = page.querySelector<HTMLElement>('.fault');

    expect(chrome?.dataset.state).toBe('loading');
    expect(surface?.hidden).toBe(true);
    expect(fault?.hidden).toBe(true);
  });
});
