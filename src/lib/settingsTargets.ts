// Which screens a link can open. A menu entry that leads to a screen this build
// does not have yet is left out, not shown dead (V3-19 AC-12).
import { SETTINGS_SECTIONS } from './settingsSections.ts'
import { PROJECT_SECTIONS } from './projectSections.ts'

/** True when Settings has a section with this id. */
export function hasSettingsSection(id: string): boolean {
  return SETTINGS_SECTIONS.some((s) => s.id === id)
}

/** True when the Project page has a section with this id. */
export function hasProjectSection(id: string): boolean {
  return PROJECT_SECTIONS.some((s) => s.id === id)
}
