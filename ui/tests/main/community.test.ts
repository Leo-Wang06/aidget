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
function page(hasGithubToken = false) {
  const root = document.createElement('section');
  document.body.append(root);
  const installed = vi.fn(async () => {});
  const page = createCommunityPage(
    root,
    () => 'en',
    installed,
    () => hasGithubToken,
  );
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
  it('keeps discovery cards compact and shows real zero downloads', async () => {
    const { root } = page();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-name')?.textContent).toBe('Clock'),
    );
    expect(root.querySelector('.community-logo-default')).toBeTruthy();
    const card = root.querySelector('article')!;
    expect(card.textContent).toContain('Clock');
    expect(card.textContent).toContain('v1.0.0');
    expect(card.textContent).not.toContain('A safe desktop clock');
    expect(card.textContent).not.toContain('JavaScript');
    expect(card.textContent).not.toContain('macOS');
    expect(card.textContent).not.toContain('Windows');
    expect(card.textContent).not.toContain('Linux');
    expect(card.textContent).not.toContain('https://api.example.com');
    await vi.waitFor(() => expect(root.textContent).toContain('0/week'));
    expect(root.querySelector('iframe')).toBeNull();
  });
  it('shows refresh progress in the button instead of the status line', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-name')).toBeTruthy());
    let resolveRefresh!: (value: CommunitySnapshot) => void;
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'community_list') {
        return new Promise<CommunitySnapshot>((resolve) => {
          resolveRefresh = resolve;
        });
      }
      return Promise.resolve([]);
    });

    const refresh = root.querySelector<HTMLButtonElement>('.community-refresh')!;
    refresh.click();

    expect(refresh.textContent).toBe('Refreshing');
    expect(refresh.disabled).toBe(true);
    expect(root.querySelector('.community-status')?.textContent).toBe('');

    resolveRefresh(structuredClone(snapshot));
    await vi.waitFor(() => expect(refresh.textContent).toBe('Refresh'));
    expect(refresh.disabled).toBe(false);
  });
  it('requires details confirmation and sends the pinned main SHA', async () => {
    const { root, installed } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    expect(dialog.textContent).toContain('A safe desktop clock');
    expect(dialog.textContent).toContain('JavaScript');
    expect(dialog.textContent).toContain('https://api.example.com');
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
  it('pins install in the dialog header and close as a text action in the footer', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    const card = root.querySelector('article')!;
    expect(card.firstElementChild?.classList.contains('community-card-heading')).toBe(true);

    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    const header = dialog.querySelector('.community-dialog-header')!;
    const content = dialog.querySelector('.community-dialog-content')!;
    const footer = dialog.querySelector('.community-dialog-footer')!;
    expect(dialog.firstElementChild).toBe(header);
    expect(dialog.lastElementChild).toBe(footer);
    expect(header.querySelector('h2')?.textContent).toBe('Clock');
    expect(header.querySelector('.community-install')).toBeTruthy();
    expect(content.querySelector('.community-install')).toBeNull();
    expect(footer.querySelector('.community-install')).toBeNull();
    const close = footer.querySelector<HTMLButtonElement>('.community-dialog-close')!;
    expect(close.textContent).toBe('Close');
    expect(close.classList.contains('secondary-button')).toBe(false);
    expect(
      [...dialog.querySelectorAll('.community-platforms li')].map((item) => item.textContent),
    ).toEqual(['macOS', 'Linux', 'Windows']);
    expect(dialog.textContent).not.toContain('360 × 220');
  });
  it('focuses the dialog itself instead of the close action', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    const close = dialog.querySelector('.community-dialog-close');
    expect(dialog.tabIndex).toBe(-1);
    expect(document.activeElement).toBe(dialog);
    expect(document.activeElement).not.toBe(close);
  });
  it('closes the dialog from the footer text action', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    (dialog.querySelector('.community-dialog-close') as HTMLButtonElement).click();
    expect(dialog.open).toBe(false);
  });
  it('places the detail description after the screenshot gallery', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const images = root.querySelector('dialog .community-detail-images')!;
    const description = root.querySelector('dialog .community-detail-description');
    expect(description).toBeTruthy();
    expect(images.nextElementSibling).toBe(description);
  });
  it('shows empty permissions and network access in the confirmation only', async () => {
    const withoutPermissions = structuredClone(snapshot);
    withoutPermissions.widgets[0].entry.config.permissions = [];
    withoutPermissions.widgets[0].entry.config.network = [];
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_list' ? Promise.resolve(withoutPermissions) : Promise.resolve([]),
    );
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-name')).toBeTruthy());
    expect(root.querySelector('article')?.textContent).not.toContain('No host permissions');
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    expect(dialog.textContent).toContain('No host permissions');
    expect(dialog.querySelector('ul.community-network')).toBeNull();
    const network = dialog.querySelector('.community-network-empty')!;
    expect(network.tagName).toBe('P');
    expect(network.textContent).toBe('No network access');
  });
  it('renders actual network origins in a list', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const network = root.querySelector('dialog ul.community-network')!;
    expect(network).toBeTruthy();
    expect(network.textContent).toContain('https://api.example.com');
    expect(root.querySelector('dialog .community-network-empty')).toBeNull();
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
      expect(root.querySelector('.community-install')?.textContent).toBe('Incompatible'),
    );
    expect((root.querySelector('.community-install') as HTMLButtonElement).disabled).toBe(true);
    expect(root.querySelector<HTMLElement>('[data-download-count]')?.hidden).toBe(true);
  });
  it('suggests configuring a GitHub token when an unauthenticated load fails', async () => {
    mocks.invoke.mockRejectedValue(new Error('HTTP 403 Forbidden'));
    const { root } = page(false);

    await vi.waitFor(() => {
      expect(root.querySelector('.community-status')?.textContent).toContain(
        'configure a GitHub token',
      );
    });
  });
  it('does not suggest a GitHub token when community loading fails with one configured', async () => {
    mocks.invoke.mockRejectedValue(new Error('HTTP 403 Forbidden'));
    const { root } = page(true);

    await vi.waitFor(() => {
      expect(root.querySelector('.community-status')?.textContent).toContain('HTTP 403 Forbidden');
    });
    expect(root.querySelector('.community-status')?.textContent).not.toContain(
      'configure a GitHub token',
    );
  });
});
