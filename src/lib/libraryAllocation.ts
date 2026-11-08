// Pure helpers behind the Allocation tab of the Rules, Agents and Commands
// screens (components/LibraryAllocationMatrix): the item × project matrix of
// `list_library_allocation`, one chip per engine the kind reaches — the Skills
// matrix (skillRows' buildAllocationMatrix) for the other libraries.
//
// A rule reaches the engines of its target, and its chips switch the whole
// rule (`apply_rule` / `remove_rule_from_project`): an engine outside the
// target shows as `off_target`. Agents and commands have one engine (Claude).

import { haystack, matchesSearch } from './listGroups.ts'

export type AllocationKind = 'rule' | 'agent' | 'command'

/** Backend states, plus `off_target`: a rule's engine its target leaves out. */
export type ItemChipState = 'installed' | 'drifted' | 'git_managed' | 'not_installed' | 'off_target'

export const ITEM_CHIP_STATES: readonly ItemChipState[] = ['installed', 'drifted', 'git_managed', 'not_installed', 'off_target']

export interface AllocationItem {
  id: string
  name: string
  description: string
  /** The engines it reaches once applied: a rule's target, Claude for the others. */
  engines: string[]
}

export interface ItemAllocation {
  /** Chip columns, in order — an open list, so the matrix never names an engine itself. */
  engines: string[]
  projects: { id: string; name: string }[]
  items: AllocationItem[]
  /** Only the chips that aren't plain "not installed, switched off". */
  chips: {
    item_id: string
    project_id: string
    engine: string
    enabled: boolean
    state: Exclude<ItemChipState, 'off_target'>
    /** Rules: a group enabled in the project keeps the rule there. */
    held: boolean
  }[]
}

export interface ItemMatrixChip {
  engine: string
  enabled: boolean
  state: ItemChipState
}

export interface ItemMatrixCell {
  projectId: string
  chips: ItemMatrixChip[]
  /** Some engine is switched on: a click switches the item off. */
  applied: boolean
  held: boolean
}

export interface ItemMatrixRow {
  item: AllocationItem
  cells: ItemMatrixCell[]
}

/** Rows in the items' order (search applied), one cell per project, one chip per engine. */
export function buildItemMatrix(allocation: ItemAllocation, search: string): ItemMatrixRow[] {
  const byKey = new Map(allocation.chips.map((c) => [`${c.item_id}\n${c.project_id}\n${c.engine}`, c]))
  return allocation.items
    .filter((item) => matchesSearch(haystack(item.name, item.description), search))
    .map((item) => ({
      item,
      cells: allocation.projects.map((p) => {
        let held = false
        const chips = allocation.engines.map((engine): ItemMatrixChip => {
          const c = byKey.get(`${item.id}\n${p.id}\n${engine}`)
          if (c?.held) held = true
          const fallback: ItemChipState = item.engines.includes(engine) ? 'not_installed' : 'off_target'
          return { engine, enabled: c?.enabled ?? false, state: c?.state ?? fallback }
        })
        return { projectId: p.id, chips, applied: chips.some((c) => c.enabled), held }
      }),
    }))
}

/**
 * Whether a chip can be clicked: never on the repo's own file or an engine the
 * rule's target leaves out, and not while a group keeps the item applied.
 */
export function itemChipSwitchable(cell: ItemMatrixCell, chip: ItemMatrixChip): boolean {
  if (chip.state === 'git_managed' || chip.state === 'off_target') return false
  return !(cell.applied && cell.held)
}
