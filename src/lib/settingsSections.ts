import {
  Cpu, Palette, Sparkles, Plug, Gauge, Radio,
  Inbox, Target, ToggleRight, Keyboard, Languages, Brain, Flag, Info, Boxes,
} from 'lucide-vue-next'
import { MessageSquareText } from 'lucide-vue-next'
import { filterByFeature, type FeatureTree } from './featureGate.ts'
import { isConnectionTypeId, type ConnectionTypeId } from './connectionTypes.ts'

/** Headings of the Settings nav, in display order. */
export const SETTINGS_GROUPS = [
  { id: 'general', labelKey: 'settings.groups.general' },
  { id: 'ai', labelKey: 'settings.groups.ai' },
  { id: 'connections', labelKey: 'settings.groups.connections' },
  { id: 'system', labelKey: 'settings.groups.system' },
] as const

export type SettingsGroupId = (typeof SETTINGS_GROUPS)[number]['id']

/**
 * Sections of the Settings screen, in nav order within their `group`. Shared by
 * SettingsView and the command palette's Settings group, so a section added here
 * is reachable from both. `feature`: hidden while that module or prop
 * (lib/featureGate) is off. Route: `/settings/<id>`; old `/settings?section=<id>`
 * links redirect there (settingsRedirect).
 */
export const SETTINGS_SECTIONS = [
  { id: 'general', group: 'general', labelKey: 'settings.sections.general', icon: Palette },
  { id: 'flags', group: 'general', labelKey: 'settings.sections.flags', icon: Flag },
  { id: 'features', group: 'general', labelKey: 'settings.sections.features', icon: ToggleRight },
  { id: 'focus', group: 'general', labelKey: 'settings.sections.focus', icon: Target, feature: 'productivity.focus' },
  // The ⌘K palette and the menu's shortcuts: two tabs (`?tab=palette|keymap`).
  { id: 'shortcuts', group: 'general', labelKey: 'settings.sections.shortcuts', icon: Keyboard, feature: 'app.command_palette' },
  { id: 'engines', group: 'ai', labelKey: 'settings.sections.engines', icon: Cpu },
  // Which models each engine may use (V3-63); every model picker filters by it.
  { id: 'model', group: 'ai', labelKey: 'settings.sections.model', icon: Boxes },
  { id: 'ai', group: 'ai', labelKey: 'settings.sections.ai', icon: Sparkles },
  { id: 'styles', group: 'ai', labelKey: 'settings.sections.styles', icon: MessageSquareText },
  { id: 'translate', group: 'ai', labelKey: 'settings.sections.translate', icon: Languages },
  { id: 'memory', group: 'ai', labelKey: 'settings.sections.memory', icon: Brain },
  // Context, quota and unattended thresholds; was "Sessions" (V3-60).
  { id: 'limits', group: 'ai', labelKey: 'settings.sections.limits', icon: Gauge },
  // GitHub, GitLab, AWS, Azure and Google accounts (lib/connectionTypes); each type
  // is gated inside the screen, and `?section=github` etc. open it filtered.
  { id: 'connections', group: 'connections', labelKey: 'settings.sections.connections', icon: Plug },
  { id: 'inbox', group: 'connections', labelKey: 'settings.sections.inbox', icon: Inbox },
  { id: 'devices', group: 'system', labelKey: 'settings.sections.devices', icon: Radio },
  { id: 'about', group: 'system', labelKey: 'settings.sections.about', icon: Info },
] as const

export type SettingsSectionId = (typeof SETTINGS_SECTIONS)[number]['id']

/** i18n key of a group's heading. */
export function groupLabelKey(group: SettingsGroupId): string {
  return SETTINGS_GROUPS.find((g) => g.id === group)!.labelKey
}

/** i18n keys of the "Settings › group › section" trail in the header / window title. */
export function settingsCrumbKeys(id: SettingsSectionId): [string, string, string] {
  const s = SETTINGS_SECTIONS.find((x) => x.id === id)!
  return ['settings.title', groupLabelKey(s.group), s.labelKey]
}

/**
 * The Settings nav: each group with its sections whose feature is on, in group
 * order. A group whose sections are all switched off is left out, so no heading
 * is drawn over nothing.
 */
export function settingsNav<S extends { group: SettingsGroupId; feature?: string }>(
  sections: readonly S[],
  tree: FeatureTree,
): { id: SettingsGroupId; labelKey: string; sections: S[] }[] {
  const visible = filterByFeature([...sections], tree)
  return SETTINGS_GROUPS
    .map((g) => ({ id: g.id, labelKey: g.labelKey, sections: visible.filter((s) => s.group === g.id) }))
    .filter((g) => g.sections.length > 0)
}

/** Where a Settings link lands: the section, plus the tab or filter it implies. */
export interface SettingsPlace {
  section: SettingsSectionId
  devicesTab?: 'remote' | 'backup'
  shortcutsTab?: 'palette' | 'keymap'
  connectionsFilter?: ConnectionTypeId
}

// Remote Control and Backup (once Sync) were sections of their own: now tabs of `devices`.
const DEVICES_TAB_OF: Record<string, 'remote' | 'backup'> = { remote: 'remote', sync: 'backup', backup: 'backup' }
// The command palette and Keyboard shortcuts were sections of their own: now tabs of `shortcuts`.
const SHORTCUTS_TAB_OF: Record<string, 'palette' | 'keymap'> = { palette: 'palette', keymap: 'keymap' }
// Claude Accounts and Engine Paths were sections of their own: now the engine cards of `engines`.
// Usage & Budget, then AI › Sessions, are now AI › Limits.
const MERGED_INTO: Record<string, SettingsSectionId> = {
  claude: 'engines', engine: 'engines', usage: 'limits', sessions: 'limits',
}

/**
 * Resolves a deep link's section id, old ids included. GitHub, GitLab, AWS, Azure
 * and Google were sections too: now Connections, filtered to that type. An unknown
 * id gives `null` (the caller keeps whatever is open).
 */
export function resolveSettingsPlace(target: unknown): SettingsPlace | null {
  if (typeof target !== 'string') return null
  const id = MERGED_INTO[target] ?? target
  if (DEVICES_TAB_OF[id]) return { section: 'devices', devicesTab: DEVICES_TAB_OF[id] }
  if (SHORTCUTS_TAB_OF[id]) return { section: 'shortcuts', shortcutsTab: SHORTCUTS_TAB_OF[id] }
  if (isConnectionTypeId(id)) return { section: 'connections', connectionsFilter: id }
  return isSettingsSectionId(id) ? { section: id } : null
}

/** True for a section id of the nav (not an old id that only redirects). */
export function isSettingsSectionId(id: unknown): id is SettingsSectionId {
  return typeof id === 'string' && SETTINGS_SECTIONS.some((s) => s.id === id)
}

/** A Settings route: `/settings/<section>` plus its query (`?tab=` of Devices, `?type=` of Connections, …). */
export interface SettingsLocation {
  path: string
  query: Record<string, string>
}

/** The route of a place; `extra` carries the caller's other query keys (`?new=1`…). */
export function settingsLocation(place: SettingsPlace, extra: Record<string, string> = {}): SettingsLocation {
  const query = { ...extra }
  if (place.devicesTab) query.tab = place.devicesTab
  if (place.shortcutsTab) query.tab = place.shortcutsTab
  if (place.connectionsFilter) query.type = place.connectionsFilter
  return { path: `/settings/${place.section}`, query }
}

/** The same route as one string, for a window without this app's router (SCREEN_OPEN_ROUTE). */
export function settingsHref(loc: SettingsLocation): string {
  const qs = new URLSearchParams(loc.query).toString()
  return qs ? `${loc.path}?${qs}` : loc.path
}

/**
 * Where an old `/settings?section=<id>` link goes: the section's route, old ids
 * through resolveSettingsPlace (`github` → Connections `?type=github`, `remote` →
 * Devices `?tab=remote`…). No or an unknown id → General. The link's other query
 * keys are kept, so the caller's intent (`?new=1`) survives.
 */
export function settingsRedirect(query: Record<string, unknown>): SettingsLocation {
  const { section, ...rest } = query
  const extra: Record<string, string> = {}
  for (const [k, v] of Object.entries(rest)) if (typeof v === 'string') extra[k] = v
  return settingsLocation(resolveSettingsPlace(section) ?? { section: 'general' }, extra)
}

/**
 * The place `/settings/:section` shows. An old id in the path resolves like a
 * deep link; an unknown one shows General. `?tab=` / `?type=` pick the Devices or
 * Shortcuts tab, or the Connections filter.
 */
export function settingsPlaceFromRoute(params: Record<string, unknown>, query: Record<string, unknown>): SettingsPlace {
  const place: SettingsPlace = { ...(resolveSettingsPlace(params.section) ?? { section: 'general' }) }
  if (place.section === 'devices' && (query.tab === 'remote' || query.tab === 'backup')) place.devicesTab = query.tab
  if (place.section === 'shortcuts' && (query.tab === 'palette' || query.tab === 'keymap')) place.shortcutsTab = query.tab
  if (place.section === 'connections' && isConnectionTypeId(query.type)) place.connectionsFilter = query.type
  return place
}
