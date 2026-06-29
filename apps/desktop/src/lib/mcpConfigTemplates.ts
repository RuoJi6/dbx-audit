export type McpEnvEntry = readonly [key: string, value: string];

function envObject(envEntries: readonly McpEnvEntry[]): Record<string, string> {
  return Object.fromEntries(envEntries);
}

export function buildMcpJsonConfig(envEntries: readonly McpEnvEntry[] = []): string {
  const dbx: Record<string, unknown> = {
    command: "dbx-mcp-server",
  };

  if (envEntries.length > 0) {
    dbx.env = envObject(envEntries);
  }

  return JSON.stringify({ mcpServers: { dbx } }, null, 2);
}

export function buildMcpVsCodeConfig(envEntries: readonly McpEnvEntry[] = []): string {
  const dbx: Record<string, unknown> = {
    type: "stdio",
    command: "dbx-mcp-server",
  };

  if (envEntries.length > 0) {
    dbx.env = envObject(envEntries);
  }

  return JSON.stringify({ servers: { dbx } }, null, 2);
}

export function buildMcpCodexConfig(envEntries: readonly McpEnvEntry[] = []): string {
  const lines = ["[mcp_servers.dbx]", 'command = "dbx-mcp-server"'];

  if (envEntries.length > 0) {
    lines.push("");
    lines.push("[mcp_servers.dbx.env]");
    for (const [key, value] of envEntries) {
      lines.push(`${key} = ${JSON.stringify(value)}`);
    }
  }

  return lines.join("\n");
}

export function buildMcpOpenCodeConfig(envEntries: readonly McpEnvEntry[] = []): string {
  const dbx: Record<string, unknown> = {
    type: "local",
    command: ["dbx-mcp-server"],
  };

  if (envEntries.length > 0) {
    dbx.environment = envObject(envEntries);
  }

  return JSON.stringify({ mcp: { dbx } }, null, 2);
}
