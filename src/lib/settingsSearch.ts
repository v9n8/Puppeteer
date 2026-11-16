// Settings search: pure index + ranking behind the "Search settings" box
// (components/settings/SettingsSearch.vue). Kept free of Vue / Tauri / `@/`
// imports (settingsSections.ts only brings its icons) so `node --test` can run it. The rows come from settingsSearchRows.ts;
// each section is found by its own name on top of them.
import { foldText } from './commandPalette.ts'
import { filterByFeature, type FeatureTree } from './featureGate.ts'
import type { SearchRow } from './settingsSearchRows.ts'
import { groupLabelKey, type SettingsGroupId, type SettingsSectionId } from './settingsSections.ts'

/** Most results the dropdown lists. */
export const MAX_SEARCH_RESULTS = 20

/** i18n key of a tab's name, for the "Group › Section › Tab" path of a result. */
export const TAB_LABEL_KEY: Partial<Record<SettingsSectionId, Record<string, string>>> = {
  shortcuts: { palette: 'settings.shortcuts.tabs.palette', keymap: 'settings.shortcuts.tabs.keymap' },
  devices: { remote: 'settings.devicesTabs.remote', backup: 'settings.devicesTabs.backup' },
  limits: {
    context: 'settings.sessions.contextTitle',
    quota: 'settings.sessions.quotaTitle',
    unattended: 'settings.sessions.unattendedTitle',
    notify: 'settings.sessions.notifyTitle',
  },
}

interface SectionLike {
  id: SettingsSectionId
  group: SettingsGroupId
  labelKey: string
  feature?: string
}

type Translate = (key: string) => string

export interface SearchEntry {
  /** Row id (`search-id`), or `section:<id>` for a section's own name. */
  id: string
  section: SettingsSectionId
  group: SettingsGroupId
  sectionLabelKey: string
  tab?: string
  feature?: string
  /** True for a section's own name (no row to scroll to). */
  isSection: boolean
  label: string
  /** Folded label, current language then English. */
  labelFolded: string
  /** Folded synonyms + section name: matched after the label. */
  extraFolded: string
}

/** Entries for every section (by name) and every declared row. */
export function settingsSearchIndex(
  sections: readonly SectionLike[],
  rows: Record<string, SearchRow[]>,
  t: Translate,
  tEn: Translate,
): SearchEntry[] {
  const out: SearchEntry[] = []
  for (const s of sections) {
    const sectionName = t(s.labelKey)
    const base = { section: s.id, group: s.group, sectionLabelKey: s.labelKey, feature: s.feature }
    out.push({
      ...base,
      id: `section:${s.id}`,
      isSection: true,
      label: sectionName,
      labelFolded: foldText(`${sectionName} ${tEn(s.labelKey)}`),
      extraFolded: '',
    })
    for (const r of rows[s.id] ?? []) {
      const label = t(r.labelKey)
      out.push({
        ...base,
        id: r.id,
        tab: r.tab,
        isSection: false,
        label,
        labelFolded: foldText(`${label} ${tEn(r.labelKey)}`),
        extraFolded: foldText(`${r.keywordsKey ? `${t(r.keywordsKey)} ${tEn(r.keywordsKey)}` : ''} ${sectionName} ${tEn(s.labelKey)}`),
      })
    }
  }
  return out
}

/**
 * Entries matching every word of `query` (case and accents ignored), the
 * sections whose feature is off left out. A word in the row's name ranks
 * first, then in its keywords or section name; a section's own name before
 * rows of equal rank. At most MAX_SEARCH_RESULTS.
 */
export function searchSettings(index: readonly SearchEntry[], query: string, tree: FeatureTree): SearchEntry[] {
  const words = foldText(query).split(/\s+/).filter(Boolean)
  if (!words.length) return []
  const visible = filterByFeature([...index], tree)
  const scored: { entry: SearchEntry; rank: number; at: number }[] = []
  visible.forEach((entry, at) => {
    if (words.every((w) => entry.labelFolded.includes(w))) scored.push({ entry, rank: entry.isSection ? 0 : 1, at })
    else if (words.every((w) => entry.labelFolded.includes(w) || entry.extraFolded.includes(w))) scored.push({ entry, rank: 2, at })
  })
  scored.sort((a, b) => a.rank - b.rank || a.at - b.at)
  return scored.slice(0, MAX_SEARCH_RESULTS).map((s) => s.entry)
}

/** "Group › Section" (+ " › Tab") of a result, in the app language. */
export function resultPath(entry: SearchEntry, t: Translate): string {
  const parts = [t(groupLabelKey(entry.group)), t(entry.sectionLabelKey)]
  const tabKey = entry.tab ? TAB_LABEL_KEY[entry.section]?.[entry.tab] : undefined
  if (tabKey) parts.push(t(tabKey))
  return parts.join(' › ')
}
