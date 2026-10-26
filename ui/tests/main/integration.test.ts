import { describe, expect, it } from 'vite-plus/test';

import {
  allInstalled,
  integrationAgents,
  mcpCommandLine,
  type IntegrationInfo,
} from '../../src/main/integration';

const info: IntegrationInfo = {
  mcpCommand: '/Applications/aidget.app/Contents/MacOS/aidget',
  mcpArgs: ['mcp'],
  skillDirectory: 'skills/aidget',
  skillContent: '---\nname: aidget\n---\n',
  agents: [
    {
      id: 'codex',
      name: 'Codex',
      icon: 'codex-color',
      mcp: { path: '/tmp/config.toml', installed: true },
      skills: { path: '/tmp/skills', installed: false },
    },
    {
      id: 'zed',
      name: 'Zed',
      icon: 'zed',
      mcp: { path: '/tmp/settings.json', installed: false },
    },
  ],
};

describe('integration configuration', () => {
  it('renders the complete MCP command', () => {
    expect(mcpCommandLine(info)).toBe('/Applications/aidget.app/Contents/MacOS/aidget mcp');
    expect(
      mcpCommandLine({
        mcpCommand: '/Applications/Aidget Beta.app/Contents/MacOS/aidget',
        mcpArgs: ['mcp', '--profile', 'local dev'],
      }),
    ).toBe('"/Applications/Aidget Beta.app/Contents/MacOS/aidget" mcp --profile "local dev"');
  });

  it('shows only Agents that can receive the current resource', () => {
    expect(integrationAgents(info, 'mcp').map((agent) => agent.id)).toEqual(['codex', 'zed']);
    expect(integrationAgents(info, 'skills').map((agent) => agent.id)).toEqual(['codex']);
    expect(allInstalled(info, 'mcp')).toBe(false);
    expect(
      allInstalled(
        {
          ...info,
          agents: [{ ...info.agents[0], mcp: { path: '/tmp/config.toml', installed: true } }],
        },
        'mcp',
      ),
    ).toBe(true);
  });
});
