export interface AgentResourceState {
  path: string;
  installed: boolean;
  conflict?: string;
}

export interface AgentIntegration {
  id: string;
  name: string;
  icon: string;
  mcp?: AgentResourceState;
  skills?: AgentResourceState;
}

export interface IntegrationInfo {
  mcpCommand: string;
  mcpArgs: string[];
  skillDirectory: string;
  skillContent: string;
  agents: AgentIntegration[];
}

export interface AgentProblem {
  agentId: string;
  resource: IntegrationResource;
  message: string;
}

export interface IntegrationMutation {
  state: IntegrationInfo;
  problems: AgentProblem[];
}

export type IntegrationResource = 'mcp' | 'skills';

/**
 * Render the complete MCP launch command with whitespace-bearing parts quoted.
 */
export function mcpCommandLine(info: Pick<IntegrationInfo, 'mcpCommand' | 'mcpArgs'>) {
  return [info.mcpCommand, ...info.mcpArgs]
    .map((part) => (/[\s"']/u.test(part) ? JSON.stringify(part) : part))
    .join(' ');
}

/** Agents that can receive one resource, as the page's icon row shows them. */
export function integrationAgents(
  info: IntegrationInfo,
  resource: IntegrationResource,
): AgentIntegration[] {
  return info.agents.filter((agent) => agent[resource] !== undefined);
}

/** Whether every Agent shown on that row has the resource installed. */
export function allInstalled(info: IntegrationInfo, resource: IntegrationResource): boolean {
  const agents = integrationAgents(info, resource).filter((agent) => !agent[resource]?.conflict);
  return agents.length > 0 && agents.every((agent) => agent[resource]?.installed);
}
