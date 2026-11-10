import {
  Cpu, Palette, FileText, Sparkles, Plug, Gauge, Radio,
  Inbox, Target, ToggleRight, Command, Keyboard, Languages,
} from 'lucide-vue-next'
import { filterByFeature, type FeatureTree } from './featureGate.ts'

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
  { id: 'translate', group: 'ai', labelKey: 'settings.sections.translate', icon: Languages },
  { id: 'usage', group: 'ai', labelKey: 'settings.sections.usage', icon: Gauge },
  { id: 'prompts', group: 'ai', labelKey: 'settings.sections.prompts', icon: FileText },
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
