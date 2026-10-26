// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import type { CommunitySnapshot } from '../../src/types/rust';
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
import { createCommunityPage, weeklyDownloads } from '../../src/main/community';
const snapshot: CommunitySnapshot = {
  commit: 'a'.repeat(40),
  widgets: [
    {
      entry: {
        package: {
          name: '@aidget-community/clock',
          version: '1.0.0',
          description: 'A safe desktop clock',
          author: 'Community',
          repository: null,
          license: 'MIT',
          files: [],
        },
        config: {
          id: 'clock',
          title: 'Clock',
          logo: null,
          screenshots: ['assets/screen.png'],
          platforms: ['macos', 'linux', 'windows'],
          size: { width: 360, height: 220 },
          dragMode: 'all',
          permissions: ['script'],
          network: ['https://api.example.com'],
        },
        files: [],
      },
      installed: false,
      idConflict: false,
      supported: true,
    },
  ],
};
beforeEach(() => {
  document.body.replaceChildren();
  mocks.invoke.mockReset();
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (
    this: HTMLDialogElement,
  ) {
    this.open = true;
  });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (
    this: HTMLDialogElement,
  ) {
    this.open = false;
    this.dispatchEvent(new Event('close'));
  });
  mocks.invoke.mockImplementation((command: string) => {
    if (command === 'community_list') {
      return Promise.resolve(structuredClone(snapshot));
    }
    if (command === 'community_downloads') {
      return Promise.resolve([{ package: '@aidget-community/clock', weekly: 0, error: null }]);
    }
    if (command === 'community_media') {
      return Promise.resolve('data:image/png;base64,iVBORw0KGgo=');
    }
    return Promise.resolve();
  });
});
function page() {
  const root = document.createElement('section');
  document.body.append(root);
  const installed = vi.fn(async () => {});
  const page = createCommunityPage(root, () => 'en', installed);
  page.show();
  return { root, page, installed };
}
describe('community cards', () => {
  it('distinguishes zero npm downloads from unavailable data', () => {
    expect(weeklyDownloads(0, 'en')).toBe('0');
    expect(weeklyDownloads(null, 'en')).toBeNull();
    expect(weeklyDownloads(16000, 'en')).toBe('16k');
    expect(weeklyDownloads(16000, 'zh')).toBe('16k');
  });
  it('shows fallback logo, permissions, operating systems and real zero', async () => {
    const { root } = page();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-name')?.textContent).toBe('Clock'),
    );
    expect(root.querySelector('.community-logo-default')).toBeTruthy();
    expect(root.textContent).toContain('JavaScript');
    expect(root.textContent).toContain('macOS');
    await vi.waitFor(() => expect(root.textContent).toContain('0/week'));
    expect(root.querySelector('iframe')).toBeNull();
  });
  it('requires details confirmation and sends the pinned main SHA', async () => {
    const { root, installed } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    expect(root.querySelector('dialog')?.open).toBe(true);
    expect(root.querySelector('dialog')?.textContent).toContain('https://api.example.com');
    expect(mocks.invoke).not.toHaveBeenCalledWith('community_install', expect.anything());
    (root.querySelector('dialog .community-install') as HTMLButtonElement).click();
    await vi.waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('community_install', {
        commit: 'a'.repeat(40),
        package: '@aidget-community/clock',
      }),
    );
    await vi.waitFor(() => expect(installed).toHaveBeenCalled());
    expect(root.querySelector('.community-install')?.textContent).toBe('Installed');
  });
  it('pins an open confirmation dialog even when the catalogue refreshes', async () => {
    const { root, page: controller } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const newer = structuredClone(snapshot);
    newer.commit = 'b'.repeat(40);
    newer.widgets[0].entry.config.network = ['https://changed.example.com'];
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_list' ? Promise.resolve(newer) : Promise.resolve([]),
    );
    controller.invalidate();
    controller.show();
    await vi.waitFor(() =>
      expect(
        mocks.invoke.mock.calls.filter(([command]) => command === 'community_list'),
      ).toHaveLength(2),
    );
    await vi.waitFor(() =>
      expect((root.querySelector('dialog .community-install') as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    expect(root.querySelector('dialog')?.textContent).toContain('https://api.example.com');
    expect(root.querySelector('dialog')?.textContent).not.toContain('https://changed.example.com');
    (root.querySelector('dialog .community-install') as HTMLButtonElement).click();
    await vi.waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('community_install', {
        commit: 'a'.repeat(40),
        package: '@aidget-community/clock',
      }),
    );
  });
  it('keeps installation failures visible and permits retry', async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_list'
        ? Promise.resolve(structuredClone(snapshot))
        : command === 'community_install'
          ? Promise.reject(new Error('integrity mismatch'))
          : Promise.resolve([]),
    );
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    (root.querySelector('dialog .community-install') as HTMLButtonElement).click();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-install-error')?.textContent).toContain(
        'integrity mismatch',
      ),
    );
    expect((root.querySelector('dialog .community-install') as HTMLButtonElement).disabled).toBe(
      false,
    );
  });
  it('disables unsupported platforms and does not fabricate download counts', async () => {
    const other = structuredClone(snapshot);
    other.widgets[0].supported = false;
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_list' ? Promise.resolve(other) : Promise.resolve([]),
    );
    const { root } = page();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-install')?.textContent).toBe('Unsupported OS'),
    );
    expect((root.querySelector('.community-install') as HTMLButtonElement).disabled).toBe(true);
    expect(root.querySelector<HTMLElement>('[data-download-count]')?.hidden).toBe(true);
  });
});
