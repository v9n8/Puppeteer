import {
  Cpu, Palette, Sparkles, Plug, Gauge, Radio,
  Inbox, Target, ToggleRight, Command, Keyboard, Languages, Brain,
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
 * (lib/featureGate) is off. Deep link: `/settings?section=<id>`.
 */
export const SETTINGS_SECTIONS = [
  { id: 'general', group: 'general', labelKey: 'settings.sections.general', icon: Palette },
  { id: 'features', group: 'general', labelKey: 'settings.sections.features', icon: ToggleRight },
  { id: 'focus', group: 'general', labelKey: 'settings.sections.focus', icon: Target, feature: 'productivity.focus' },
  { id: 'palette', group: 'general', labelKey: 'settings.sections.palette', icon: Command, feature: 'app.command_palette' },
  { id: 'keymap', group: 'general', labelKey: 'settings.sections.keymap', icon: Keyboard, feature: 'app.command_palette' },
  { id: 'engines', group: 'ai', labelKey: 'settings.sections.engines', icon: Cpu },
  { id: 'ai', group: 'ai', labelKey: 'settings.sections.ai', icon: Sparkles },
  { id: 'styles', group: 'ai', labelKey: 'settings.sections.styles', icon: MessageSquareText },
  { id: 'translate', group: 'ai', labelKey: 'settings.sections.translate', icon: Languages },
  { id: 'memory', group: 'ai', labelKey: 'settings.sections.memory', icon: Brain },
  { id: 'sessions', group: 'ai', labelKey: 'settings.sections.sessions', icon: Gauge },
  // GitHub, GitLab, AWS, Azure and Google accounts (lib/connectionTypes); each type
  // is gated inside the screen, and `?section=github` etc. open it filtered.
  { id: 'connections', group: 'connections', labelKey: 'settings.sections.connections', icon: Plug },
  { id: 'inbox', group: 'connections', labelKey: 'settings.sections.inbox', icon: Inbox },
  { id: 'devices', group: 'system', labelKey: 'settings.sections.devices', icon: Radio },
] as const

export type SettingsSectionId = (typeof SETTINGS_SECTIONS)[number]['id']

/** i18n key of a group's heading. */
export function groupLabelKey(group: SettingsGroupId): string {
  return SETTINGS_GROUPS.find((g) => g.id === group)!.labelKey
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

/** Where a `/settings?section=` deep link lands: the section, plus the tab or filter it implies. */
export interface SettingsPlace {
  section: SettingsSectionId
  devicesTab?: 'remote' | 'backup'
  connectionsFilter?: ConnectionTypeId
}

// Remote Control and Backup (once Sync) were sections of their own: now tabs of `devices`.
const DEVICES_TAB_OF: Record<string, 'remote' | 'backup'> = { remote: 'remote', sync: 'backup', backup: 'backup' }
// Claude Accounts and Engine Paths were sections of their own: now the engine cards of `engines`.
// Usage & Budget is now AI › Sessions.
const MERGED_INTO: Record<string, SettingsSectionId> = { claude: 'engines', engine: 'engines', usage: 'sessions' }

/**
 * Resolves a deep link's section id, old ids included. GitHub, GitLab, AWS, Azure
 * and Google were sections too: now Connections, filtered to that type. An unknown
 * id gives `null` (the caller keeps whatever is open).
 */
export function resolveSettingsPlace(target: unknown): SettingsPlace | null {
  if (typeof target !== 'string') return null
  const id = MERGED_INTO[target] ?? target
  if (DEVICES_TAB_OF[id]) return { section: 'devices', devicesTab: DEVICES_TAB_OF[id] }
  if (isConnectionTypeId(id)) return { section: 'connections', connectionsFilter: id }
  return SETTINGS_SECTIONS.some((s) => s.id === id) ? { section: id as SettingsSectionId } : null
}
