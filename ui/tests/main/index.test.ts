/// @vitest-environment happy-dom

import { readFileSync } from 'node:fs';

import type { EventCallback } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import type { IntegrationInfo } from '../../src/main/integration';
import type { MainState } from '../../src/main/model';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(async (_event: string, _handler: EventCallback<MainState>) => () => {}),
  openUrl: vi.fn(async () => {}),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: mocks.openUrl }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    startDragging: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
  }),
}));
vi.mock('@tauri-apps/plugin-autostart', () => ({
  disable: vi.fn(async () => {}),
  enable: vi.fn(async () => {}),
  isEnabled: vi.fn(async () => false),
}));
vi.mock('lucide', () => {
  const icon = {};
  return {
    Archive: icon,
    ArrowUpDown: icon,
    Info: icon,
    LayoutGrid: icon,
    Minus: icon,
    RotateCcw: icon,
    ServerCog: icon,
    Settings: icon,
    Square: icon,
    Trash: icon,
    UsersRound: icon,
    X: icon,
    createElement: () => document.createElement('svg'),
    createIcons: vi.fn(),
  };
});

const mainHtml = readFileSync('ui/main.html', 'utf8');

function state(lastPage: MainState['lastPage']): MainState {
  return {
    widgets: [],
    deletedWidgets: [],
    broken: [],
    version: '0.1.0',
    theme: 'system',
    language: 'system',
    trayClickAction: 'window',
    lastPage,
  };
}

function integrationState(installed: boolean): IntegrationInfo {
  return {
    mcpCommand: '/Applications/aidget.app/Contents/MacOS/aidget',
    mcpArgs: ['mcp'],
    skillDirectory: '/Users/test/.aidget/skills/aidget',
    skillContent: '',
    agents: [
      {
        id: 'codex',
        name: 'Codex',
        icon: 'codex-color',
        mcp: { path: '/Users/test/.codex/config.toml', installed },
      },
    ],
  };
}

function prepareDocument() {
  const bodyStart = mainHtml.indexOf('<body>') + '<body>'.length;
  const bodyEnd = mainHtml.lastIndexOf('</body>');
  document.body.innerHTML = mainHtml
    .slice(bodyStart, bodyEnd)
    .replace('<script type="module" src="./src/main/index.ts"></script>', '');
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
    },
  );
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    callback(0);
    return 1;
  });
}

async function loadMain(initial: MainState) {
  mocks.invoke.mockImplementation((command: string, args?: { preferences?: unknown }) => {
    if (command === 'main_state') {
      return Promise.resolve(initial);
    }
    if (command === 'main_painted') {
      return Promise.resolve(true);
    }
    if (command === 'set_preferences') {
      return Promise.resolve();
    }
    return Promise.resolve(args);
  });
  await import('../../src/main');
  await vi.waitFor(() => {
    expect(document.querySelector(`[data-page="${initial.lastPage}"]`)?.classList).toContain(
      'is-active',
    );
  });
}

function pageButton(page: string) {
  return document.querySelector<HTMLButtonElement>(`[data-page="${page}"]`)!;
}

beforeEach(() => {
  vi.resetModules();
  mocks.invoke.mockReset();
  mocks.listen.mockClear();
  mocks.openUrl.mockReset().mockResolvedValue(undefined);
  prepareDocument();
});

describe('main window navigation', () => {
  it('clears broken-widget errors after the configuration is repaired', async () => {
    const initial = state('about');
    initial.broken = [{ id: 'clock', error: 'invalid config' }];
    await loadMain(initial);
    const fault = document.querySelector<HTMLElement>('[data-fault]')!;
    expect(fault.hidden).toBe(false);
    expect(fault.textContent).toContain('clock: invalid config');

    const onState = mocks.listen.mock.calls.find(([name]) => name === 'aidget://main')![1];
    onState({ event: 'aidget://main', id: 0, payload: state('about') });
    expect(fault.hidden).toBe(true);
  });

  it('preserves an operation error while broken-widget state changes', async () => {
    const initial = state('about');
    initial.broken = [{ id: 'clock', error: 'invalid config' }];
    await loadMain(initial);
    mocks.openUrl.mockRejectedValueOnce(new Error('browser unavailable'));
    document.querySelector<HTMLButtonElement>('[data-github]')!.click();
    const fault = document.querySelector<HTMLElement>('[data-fault]')!;
    await vi.waitFor(() => expect(fault.textContent).toContain('browser unavailable'));

    const onState = mocks.listen.mock.calls.find(([name]) => name === 'aidget://main')![1];
    onState({ event: 'aidget://main', id: 0, payload: initial });
    expect(fault.textContent).toContain('browser unavailable');
    onState({ event: 'aidget://main', id: 0, payload: state('about') });
    expect(fault.hidden).toBe(false);
    expect(fault.textContent).toContain('browser unavailable');
  });

  it('restores the persisted page before the first reveal', async () => {
    await loadMain(state('about'));

    expect(pageButton('about').getAttribute('aria-current')).toBe('page');
    expect(document.querySelector<HTMLElement>('[data-view="about"]')!.hidden).toBe(false);
  });

  it('serializes rapid navigation writes so the last page wins', async () => {
    let finishFirstWrite: (() => void) | undefined;
    const written: string[] = [];
    mocks.invoke.mockImplementation(
      (
        command: string,
        args?: {
          preferences?: { lastPage: string };
        },
      ) => {
        if (command === 'main_state') {
          return Promise.resolve(state('integrations'));
        }
        if (command === 'main_painted') {
          return Promise.resolve(true);
        }
        if (command === 'integration_state') {
          return Promise.resolve({
            agents: [],
            mcpArgs: [],
            mcpCommand: 'aidget mcp',
            skillContent: '',
            skillDirectory: '',
          });
        }
        if (command === 'set_preferences') {
          written.push(args!.preferences!.lastPage);
          if (written.length === 1) {
            return new Promise<void>((resolve) => {
              finishFirstWrite = resolve;
            });
          }
          return Promise.resolve();
        }
        return Promise.resolve();
      },
    );

    await import('../../src/main');
    await vi.waitFor(() => {
      expect(pageButton('integrations').classList).toContain('is-active');
    });

    pageButton('local-widgets').click();
    await vi.waitFor(() => expect(written).toHaveLength(1));

    pageButton('about').click();
    await vi.waitFor(() => expect(pageButton('about').classList).toContain('is-active'));

    finishFirstWrite!();
    await vi.waitFor(() => expect(written).toHaveLength(2));
    expect(written.at(-1)).toBe('about');
  });

  it('opens the GitHub repository from the about page', async () => {
    await loadMain(state('about'));

    expect(document.querySelector('[data-message="discord"]')).toBeNull();
    const github = document.querySelector<HTMLButtonElement>('[data-github]')!;
    expect(github.disabled).toBe(false);

    github.click();
    await vi.waitFor(() => {
      expect(mocks.openUrl).toHaveBeenCalledWith('https://github.com/aidget-ai/aidget');
    });
  });

  it('updates an Agent chip after a successful integration change', async () => {
    let finishChange: ((state: IntegrationInfo) => void) | undefined;
    mocks.invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === 'main_state') {
        return Promise.resolve(state('integrations'));
      }
      if (command === 'main_painted') {
        return Promise.resolve(true);
      }
      if (command === 'integration_state') {
        return Promise.resolve(integrationState(false));
      }
      if (command === 'set_agent_integration') {
        return new Promise((resolve) => {
          finishChange = (next) => resolve({ state: next, problems: [] });
        });
      }
      return Promise.resolve(args);
    });

    await import('../../src/main');
    const codex = await vi.waitFor(() => {
      const button = document.querySelector<HTMLButtonElement>('[data-agent="codex"]');
      expect(button).not.toBeNull();
      return button!;
    });
    codex.click();
    await vi.waitFor(() => {
      expect(document.querySelector('[data-view="integrations"]')!.getAttribute('aria-busy')).toBe(
        'true',
      );
    });
    expect(mocks.invoke).toHaveBeenCalledWith('set_agent_integration', {
      change: { agentIds: ['codex'], resource: 'mcp', enabled: true },
    });

    finishChange!(integrationState(true));
    await vi.waitFor(() => {
      expect(document.querySelector('[data-view="integrations"]')!.getAttribute('aria-busy')).toBe(
        'false',
      );
    });
    expect(
      document
        .querySelector<HTMLButtonElement>('[data-agent="codex"]')!
        .classList.contains('is-on'),
    ).toBe(true);
  });
});
