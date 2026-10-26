/// @vitest-environment happy-dom

import { describe, expect, it, vi } from 'vite-plus/test';

import { renderAgentStrip } from '../../src/main/agent-controls';
import type { IntegrationInfo } from '../../src/main/integration';

const info: IntegrationInfo = {
  mcpCommand: 'aidget',
  mcpArgs: ['mcp'],
  skillDirectory: '/tmp/skills',
  skillContent: '',
  agents: [
    {
      id: 'codex',
      name: 'Codex',
      icon: 'codex-color',
      mcp: { path: '/tmp/codex.toml', installed: false },
    },
    {
      id: 'cursor',
      name: 'Cursor',
      icon: 'cursor',
      mcp: { path: '/tmp/cursor.json', installed: false },
    },
  ],
};

describe('Agent controls', () => {
  it('renders color logos as images and monochrome logos as masks', () => {
    const root = document.createElement('div');

    renderAgentStrip(
      root,
      info,
      'mcp',
      {
        all: 'All',
        empty: 'No Agents',
        installFor: 'Install for',
        removeFrom: 'Remove from',
      },
      false,
      vi.fn(),
    );

    const codex = root.querySelector<HTMLElement>('[data-agent="codex"]')!;
    expect(codex.querySelector('img')).not.toBeNull();
    expect(codex.querySelector('.agent-icon-mask')).toBeNull();

    const cursor = root.querySelector<HTMLElement>('[data-agent="cursor"]')!;
    expect(cursor.querySelector('img')).toBeNull();
    expect(
      cursor
        .querySelector<HTMLElement>('.agent-icon-mask')!
        .style.getPropertyValue('--agent-icon-url'),
    ).not.toBe('');
  });
});
