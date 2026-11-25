// Library (LH): the hooks and plugins Claude Code's settings files declare
// (commands/library_disk/config.rs), shown read-only beside the files. Not
// files: a row is one hook entry or one plugin, its `id` the Library's `path`.
// The scan is local-only (a hook's command can carry a token), so the kinds
// are left out in remote mode. Pure helpers of LibraryView.vue and
// components/library/.
import { haystack, matchesSearch } from './listGroups.ts'

/** Local-only. */
export const CMD_LIBRARY_CONFIG_SCAN = 'library_config_scan'

export type ConfigItemKind = 'hook' | 'plugin'
/** Display order, after the file kinds. */
export const CONFIG_ITEM_KINDS: readonly ConfigItemKind[] = ['hook', 'plugin']

/** The settings level: `~/.claude/settings.json`, `.claude/settings.json`, `.claude/settings.local.json`. */
export type ConfigSource = 'user' | 'project' | 'local'

export interface HookInfo {
  /** Empty on the notice row of a file that sets `disableAllHooks` and lists no hook. */
  event: string
  matcher: string | null
  /** `command`, `prompt`, … as written. */
  hook_type: string
  /** The command or prompt; another type's whole entry as JSON. */
  command: string
  timeout: number | null
  /** Its file sets `disableAllHooks: true`. */
  all_disabled: boolean
}

export interface PluginParts {
  skills: string[]
  commands: string[]
  agents: string[]
  hook_events: string[]
  /** Names only: managed under MCP. */
  mcp_servers: string[]
  /** `plugin.json` keys naming other paths, not followed: the lists may miss parts. */
  custom_paths: string[]
}

export interface PluginInfo {
  /** `name@marketplace`. */
  key: string
  marketplace: string | null
  /** The most specific file's value; null when no file read sets it. */
  enabled: boolean | null
  installed: boolean
  version: string | null
  install_path: string | null
  install_scope: string | null
  parts: PluginParts | null
}

/** What `library_config_scan` returns per row. */
export interface ConfigItem {
  kind: ConfigItemKind
  id: string
  name: string
  description: string
  source: ConfigSource
  /** The settings file it comes from (`installed_plugins.json` for a plugin no file sets). */
  file: string
  /** Why it can't be shown; never the file's content. */
  error: string | null
  hook: HookInfo | null
  plugin: PluginInfo | null
}

/** The row a file gets when it turns every hook off and lists none. */
export function isHooksOffNotice(item: Pick<ConfigItem, 'hook'>): boolean {
  return !!item.hook && item.hook.event === ''
}

export function isConfigKind(kind: string): kind is ConfigItemKind {
  return (CONFIG_ITEM_KINDS as readonly string[]).includes(kind)
}

/** A list row of a config kind; `config` tells it apart from a file row. */
export interface ConfigRow {
  id: string
  config: ConfigItem
}

/** The rows of one kind that match `search`, by name then id. */
export function configRows(items: readonly ConfigItem[], kind: ConfigItemKind, search: string): ConfigRow[] {
  return items
    .filter((i) => i.kind === kind && matchesSearch(haystack(i.name, i.description, i.plugin?.key), search))
    .sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id))
    .map((config) => ({ id: config.id, config }))
}

export type PluginState = 'on' | 'off' | 'unset'

export function pluginState(plugin: Pick<PluginInfo, 'enabled'>): PluginState {
  if (plugin.enabled === null) return 'unset'
  return plugin.enabled ? 'on' : 'off'
}

/** The parts of a plugin, as label key + names, skipping the empty ones. */
export function pluginPartGroups(parts: PluginParts): { key: keyof PluginParts; names: string[] }[] {
  const order: (keyof PluginParts)[] = ['skills', 'commands', 'agents', 'hook_events', 'mcp_servers']
  return order.map((key) => ({ key, names: parts[key] })).filter((g) => g.names.length > 0)
}

/**
 * When the scan itself fails (no project folder, …): one error row per kind,
 * so the tabs say so while the files stay listed. `name` titles the rows,
 * `source` is the tab's level (`user` on Global, `project` on a project).
 */
export function configScanErrorRows(error: string, name: string, source: ConfigSource): ConfigItem[] {
  return CONFIG_ITEM_KINDS.map((kind) => ({
    kind,
    id: `${kind}-error:scan`,
    name,
    description: '',
    source,
    file: '',
    error,
    hook: null,
    plugin: null,
  }))
}

/**
 * i18n key of the source badge's hint: a Plugins row from the install list (a
 * plugin no settings file sets, or that list's error row) names that list.
 */
export function sourceHintKey(item: Pick<ConfigItem, 'kind' | 'source' | 'file'>): string {
  if (item.kind === 'plugin' && /[\\/]installed_plugins\.json$/.test(item.file)) return 'library.config.sourceHint.installed'
  return `library.config.sourceHint.${item.source}`
}
