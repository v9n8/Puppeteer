// Tool groups of the built-in `puppeteer` server that a feature switch can hide.
// The app lists the switched-off groups in env PUPPETEER_MCP_DISABLED_GROUPS
// (`builtin_gates` in src-tauri/src/commands/mcp.rs). A hidden tool is left out
// of tools/list and answers tools/call as an unknown tool.

const PREFIXES = { notes: 'notes_', todos: 'todos_', slack: 'slack_threads_', vps: 'vps_' };

/** `"todos, vps"` → Set {todos, vps}. Unknown names are kept but match nothing. */
export function disabledGroups(value) {
  return new Set(String(value ?? '').split(',').map((g) => g.trim()).filter(Boolean));
}

/** The group a tool belongs to, or null for tools no switch can hide. */
export function toolGroup(name) {
  return Object.keys(PREFIXES).find((g) => name.startsWith(PREFIXES[g])) ?? null;
}

/** Remove the tools of `disabled` groups from the registry, in place. */
export function dropGroups(tools, disabled) {
  for (const name of Object.keys(tools)) {
    if (disabled.has(toolGroup(name))) delete tools[name];
  }
  return tools;
}
