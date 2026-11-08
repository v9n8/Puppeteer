// Tool groups of the built-in `puppeteer` server that a feature switch can hide.
// The app lists the switched-off groups in env PUPPETEER_MCP_DISABLED_GROUPS
// (`builtin_gates` in src-tauri/src/commands/mcp.rs). A hidden tool is left out
// of tools/list and answers tools/call as an unknown tool.

const PREFIXES = { notes: 'notes_', todos: 'todos_', slack: 'slack_threads_', vps: 'vps_', schedules: 'schedule_' };
// A tool serving several groups: hidden only once all of them are off.
// `git_accounts_list` has no group: code hosting is core.
const SHARED = { cloud_accounts_list: ['aws', 'azure'] };

/** `"todos, vps"` → Set {todos, vps}. Unknown names are kept but match nothing. */
export function disabledGroups(value) {
  return new Set(String(value ?? '').split(',').map((g) => g.trim()).filter(Boolean));
}

/** The group a tool belongs to, or null for tools no switch can hide. */
export function toolGroup(name) {
  return Object.keys(PREFIXES).find((g) => name.startsWith(PREFIXES[g])) ?? null;
}

/** Whether `disabled` hides the tool `name`. */
export function hidden(name, disabled) {
  const shared = SHARED[name];
  return shared ? shared.every((g) => disabled.has(g)) : disabled.has(toolGroup(name));
}

/** Remove the tools of `disabled` groups from the registry, in place. */
export function dropGroups(tools, disabled) {
  for (const name of Object.keys(tools)) {
    if (hidden(name, disabled)) delete tools[name];
  }
  return tools;
}

/** `{ aws: [...], azure: [...] }` with the rows of a disabled group emptied. */
export function keepEnabled(byGroup, disabled) {
  return Object.fromEntries(Object.entries(byGroup).map(([g, rows]) => [g, disabled.has(g) ? [] : rows]));
}
