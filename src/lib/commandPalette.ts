// Command palette (⌘K, prop `app.command_palette`): its stored layout, which
// items it shows and the accent-insensitive fuzzy search. Kept free of Vue /
// Tauri / `@/` imports so the helpers can be unit-tested with `node --test`;
// components/CommandPalette.vue builds the items and runs them.
//
// Pins and hides are stored by item id (`go./skills`, `project:<uuid>`), never
// by the label on screen, so renaming a project or switching the language
// keeps them.
import { allowsFeature, type FeatureTree } from './featureGate.ts'

export const PALETTE_FEATURE = 'app.command_palette'

/**
 * Settings row holding the whole layout as JSON. Every `palette.*` row travels
 * in a settings backup (`PORTABLE_SETTING_PREFIXES`, settings.rs).
 */
export const PALETTE_SETTING_KEY = 'palette.config'

/** The groups, in their default order. */
export const PALETTE_GROUPS = ['nav', 'session', 'project', 'prompt', 'settings'] as const
export type PaletteGroupId = (typeof PALETTE_GROUPS)[number]

export interface PaletteGroupConfig {
  id: PaletteGroupId
  enabled: boolean
}

export interface PaletteConfig {
  /** Every group exactly once, in display order. */
  groups: PaletteGroupConfig[]
  /** Item ids listed first, in the order they were pinned. */
  pinned: string[]
  /** Item ids never listed. */
  hidden: string[]
}

/** The parts Settings can restore to their default one by one. */
export type PaletteConfigPart = 'groups' | 'pinned' | 'hidden'

export function defaultPaletteConfig(): PaletteConfig {
  return { groups: PALETTE_GROUPS.map((id) => ({ id, enabled: true })), pinned: [], hidden: [] }
}

function isGroupId(v: unknown): v is PaletteGroupId {
  return typeof v === 'string' && (PALETTE_GROUPS as readonly string[]).includes(v)
}

function idList(v: unknown): string[] {
  if (!Array.isArray(v)) return []
  return [...new Set(v.filter((x): x is string => typeof x === 'string' && x.length > 0))]
}

/**
 * Parse the `palette.config` setting. Never throws: a missing, malformed or
 * hand-edited value degrades to the defaults part by part. Unknown groups are
 * dropped and groups added by a later build are appended, on.
 */
export function parsePaletteConfig(raw: string | null | undefined): PaletteConfig {
  const config = defaultPaletteConfig()
  if (!raw) return config
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    return config
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return config
  const { groups, pinned, hidden } = parsed as Record<string, unknown>

  if (Array.isArray(groups)) {
    const seen = new Set<PaletteGroupId>()
    const stored: PaletteGroupConfig[] = []
    for (const g of groups) {
      const id = g && typeof g === 'object' ? (g as Record<string, unknown>).id : undefined
      if (!isGroupId(id) || seen.has(id)) continue
      seen.add(id)
      stored.push({ id, enabled: (g as Record<string, unknown>).enabled !== false })
    }
    config.groups = [...stored, ...config.groups.filter((g) => !seen.has(g.id))]
  }
  config.hidden = idList(hidden)
  config.pinned = idList(pinned).filter((id) => !config.hidden.includes(id))
  return config
}

export function serializePaletteConfig(config: PaletteConfig): string {
  return JSON.stringify(config)
}

/** `config` with one part back to its default, the others kept. */
export function resetPalettePart(config: PaletteConfig, part: PaletteConfigPart): PaletteConfig {
  return { ...config, [part]: defaultPaletteConfig()[part] }
}

/** Move group `id` by `delta` places (clamped to the list). */
export function moveGroup(config: PaletteConfig, id: PaletteGroupId, delta: number): PaletteConfig {
  const groups = [...config.groups]
  const from = groups.findIndex((g) => g.id === id)
  if (from === -1) return config
  const to = Math.max(0, Math.min(groups.length - 1, from + delta))
  groups.splice(to, 0, ...groups.splice(from, 1))
  return { ...config, groups }
}

export function setGroupEnabled(config: PaletteConfig, id: PaletteGroupId, enabled: boolean): PaletteConfig {
  return { ...config, groups: config.groups.map((g) => (g.id === id ? { ...g, enabled } : g)) }
}

/** Pin `id` (last), or unpin it when it is pinned. */
export function togglePinned(config: PaletteConfig, id: string): PaletteConfig {
  const pinned = config.pinned.includes(id) ? config.pinned.filter((p) => p !== id) : [...config.pinned, id]
  return { ...config, pinned }
}

/** Hide `id`; a hidden item is unpinned too, so it never comes back pinned. */
export function hideItem(config: PaletteConfig, id: string): PaletteConfig {
  if (config.hidden.includes(id)) return config
  return { ...config, hidden: [...config.hidden, id], pinned: config.pinned.filter((p) => p !== id) }
}

export function unhideItem(config: PaletteConfig, id: string): PaletteConfig {
  return { ...config, hidden: config.hidden.filter((h) => h !== id) }
}

// ── Items ───────────────────────────────────────────────────────────────────

export interface PaletteItem {
  /** Stable id: what pins and hides are stored by. */
  id: string
  group: PaletteGroupId
  label: string
  /** Shown dimmed after the label and searched too: a project name, a body preview. */
  detail?: string
  /** Module or prop id (lib/featureGate) the item belongs to: while it is off the item is gone. */
  feature?: string
}

export interface PaletteSection<T extends PaletteItem> {
  /** `pinned`, or the group the items belong to. */
  id: 'pinned' | PaletteGroupId
  items: T[]
}

/** Items the palette may list: feature on, group on, not hidden. A pin never overrides any of these. */
export function visibleItems<T extends PaletteItem>(items: readonly T[], config: PaletteConfig, tree: FeatureTree): T[] {
  const groupOn = new Set(config.groups.filter((g) => g.enabled).map((g) => g.id))
  const hidden = new Set(config.hidden)
  return items.filter((item) => groupOn.has(item.group) && !hidden.has(item.id) && allowsFeature(tree, item.feature))
}

/** Best of a match on the label and one on label + detail (worth less). */
function itemScore(query: string, item: PaletteItem): number | null {
  const label = fuzzyScore(query, item.label)
  if (label !== null) return label
  if (!item.detail) return null
  const both = fuzzyScore(query, `${item.label} ${item.detail}`)
  return both === null ? null : both / 2
}

/**
 * What the palette lists for `query`: the pinned items first (pin order), then
 * each group that is on, in the configured order; a pinned item is not listed
 * twice. With a query only matches are kept, best first within each section.
 * Empty sections are left out.
 */
export function paletteSections<T extends PaletteItem>(
  items: readonly T[],
  config: PaletteConfig,
  tree: FeatureTree,
  query: string,
): PaletteSection<T>[] {
  const shown = visibleItems(items, config, tree)
  const byId = new Map(shown.map((item) => [item.id, item]))
  const pinned = config.pinned.flatMap((id) => byId.get(id) ?? [])
  const pinnedIds = new Set(pinned.map((item) => item.id))
  const sections: PaletteSection<T>[] = [
    { id: 'pinned', items: pinned },
    ...config.groups
      .filter((g) => g.enabled)
      .map((g) => ({ id: g.id, items: shown.filter((item) => item.group === g.id && !pinnedIds.has(item.id)) })),
  ]
  const q = query.trim()
  return sections
    .map((section) => {
      if (!q) return section
      const scored = section.items.flatMap((item) => {
        const score = itemScore(q, item)
        return score === null ? [] : [{ item, score }]
      })
      // Array.prototype.sort is stable: equal scores keep the section's order.
      scored.sort((a, b) => b.score - a.score)
      return { ...section, items: scored.map((s) => s.item) }
    })
    .filter((section) => section.items.length > 0)
}

// ── Search ──────────────────────────────────────────────────────────────────

/**
 * Lower-case, accents dropped: "Cài đặt" → "cai dat". `đ` is a letter of its
 * own in Unicode (no decomposition), so it is mapped by hand.
 */
export function foldText(text: string): string {
  return text.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().replace(/đ/g, 'd')
}

const isWordStart = (hay: string, i: number) => i === 0 || !/[a-z0-9]/.test(hay[i - 1])

function tokenScore(token: string, hay: string): number | null {
  const at = hay.indexOf(token)
  if (at !== -1) {
    // A substring beats any scattered match; at a word start, and early, better.
    return 200 + (isWordStart(hay, at) ? 100 : 0) - Math.min(at, 50) + token.length
  }
  // Letters in order: each gap costs, each letter at a word start counts.
  let score = 100
  let prev = -1
  for (const ch of token) {
    const i = hay.indexOf(ch, prev + 1)
    if (i === -1) return null
    if (prev !== -1) score -= Math.min(i - prev - 1, 10)
    if (isWordStart(hay, i)) score += 5
    prev = i
  }
  return Math.max(score, 1)
}

/**
 * How well `query` matches `text`, accents and case ignored, or null when it
 * does not. Each word of the query must match on its own — as a substring, or
 * its letters in order ("nss" finds "New session") — so word order is free.
 * An empty query matches everything with 0.
 */
export function fuzzyScore(query: string, text: string): number | null {
  const tokens = foldText(query).split(/\s+/).filter(Boolean)
  if (!tokens.length) return 0
  const hay = foldText(text)
  let total = 0
  for (const token of tokens) {
    const s = tokenScore(token, hay)
    if (s === null) return null
    total += s
  }
  return total
}

// ── Shortcut ────────────────────────────────────────────────────────────────

/**
 * ⌘K on macOS, Ctrl+K elsewhere. Only the platform's own modifier: Ctrl+K in a
 * macOS text field is "delete to end of line".
 */
export function isPaletteShortcut(
  e: { key: string; metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean },
  isMac: boolean,
): boolean {
  if (e.shiftKey || e.altKey) return false
  const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey
  return mod && (e.key === 'k' || e.key === 'K')
}
