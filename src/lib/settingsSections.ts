import {
  Cpu, Palette, FileText, Sparkles, Github, Gitlab, Cloud, CloudCog, Gauge, Radio,
  HardDrive, UserCircle, Inbox, Target, ToggleRight, Command, Keyboard,
} from 'lucide-vue-next'

/**
 * Sections of the Settings screen, in nav order. Shared by SettingsView and the
 * command palette's Settings group, so a section added here is reachable from
 * both. `feature`: hidden while that module or prop (lib/featureGate) is off.
 * Deep link: `/settings?section=<id>`.
 */
export const SETTINGS_SECTIONS = [
  { id: 'general', labelKey: 'settings.sections.general', icon: Palette },
  { id: 'features', labelKey: 'settings.sections.features', icon: ToggleRight },
  { id: 'focus', labelKey: 'settings.sections.focus', icon: Target, feature: 'productivity.focus' },
  { id: 'github', labelKey: 'settings.sections.github', icon: Github },
  { id: 'gitlab', labelKey: 'settings.sections.gitlab', icon: Gitlab },
  { id: 'aws', labelKey: 'settings.sections.aws', icon: Cloud, feature: 'infra_access.aws' },
  { id: 'azure', labelKey: 'settings.sections.azure', icon: CloudCog, feature: 'infra_access.azure' },
  { id: 'claude', labelKey: 'settings.sections.claude', icon: UserCircle },
  { id: 'engine', labelKey: 'settings.sections.engine', icon: Cpu },
  { id: 'ai', labelKey: 'settings.sections.ai', icon: Sparkles },
  { id: 'google', labelKey: 'settings.sections.google', icon: HardDrive, feature: 'integrations.google' },
  { id: 'usage', labelKey: 'settings.sections.usage', icon: Gauge },
  { id: 'devices', labelKey: 'settings.sections.devices', icon: Radio },
  { id: 'inbox', labelKey: 'settings.sections.inbox', icon: Inbox },
  { id: 'prompts', labelKey: 'settings.sections.prompts', icon: FileText },
  { id: 'palette', labelKey: 'settings.sections.palette', icon: Command, feature: 'app.command_palette' },
  { id: 'keymap', labelKey: 'settings.sections.keymap', icon: Keyboard, feature: 'app.command_palette' },
] as const

export type SettingsSectionId = (typeof SETTINGS_SECTIONS)[number]['id']
