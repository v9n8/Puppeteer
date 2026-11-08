// Pure helpers behind the Agents and Commands screens (one screen per kind,
// views/LibraryItemsView): the sidebar rows, each project's link state for the
// selected item, and the file's frontmatter.
//
// An agent is named by its frontmatter `name:` (Claude Code reads it there), so
// the name field rewrites that line and the file name follows it. A slash
// command has no `name:` — its file name is the command — so its name is a
// field of its own. Both keep `description:` in the frontmatter.
import { GROUP_NONE, haystack, matchesSearch, type RowGroup } from './listGroups.ts'
import { parseFrontmatter, setFrontmatterField } from './rulesRows.ts'

export type LibraryKind = 'agent' | 'command'

/** What a project holds where an applied item should be linked (`get_applied_library_items`). */
export type LinkState = 'installed' | 'drifted' | 'git_managed'

/** The prop gating each kind (backend `LibraryKind::feature`). */
export const KIND_FEATURE: Readonly<Record<LibraryKind, string>> = {
  agent: 'library.agents',
  command: 'library.commands',
}

export const AGENT_TEMPLATE = `---
name: my-agent
description: When Claude should hand a task to this agent
---

You are a specialist in … Describe the agent's role, how it works and what it returns.
`

export const COMMAND_TEMPLATE = `---
description: What this command does
---

Describe the task. $ARGUMENTS is replaced with whatever follows the command.
`

export function templateOf(kind: LibraryKind): string {
  return kind === 'agent' ? AGENT_TEMPLATE : COMMAND_TEMPLATE
}

export interface ItemLike {
  id: string
  name: string
  description: string
}

export interface AppliedLike {
  item_id: string
  state: LinkState
}

export interface ItemRow<I extends ItemLike = ItemLike> {
  id: string
  item: I
  /** Projects the item is applied to. */
  projects: number
  /** Of those, the ones that hold something else at its link. */
  drifted: number
}

/**
 * Filters the items (name + description) into the sidebar's single, header-less
 * folder, each with how many projects it is applied to. `applied` maps a
 * project id to that project's applied items; projects not loaded are skipped.
 */
export function buildItemRows<I extends ItemLike>(
  items: readonly I[],
  opts: { search: string; applied: Readonly<Record<string, readonly AppliedLike[] | undefined>>; label: string },
): RowGroup<ItemRow<I>>[] {
  const counts = new Map<string, { projects: number; drifted: number }>()
  for (const list of Object.values(opts.applied)) {
    for (const a of list ?? []) {
      const c = counts.get(a.item_id) ?? { projects: 0, drifted: 0 }
      c.projects++
      if (a.state === 'drifted') c.drifted++
      counts.set(a.item_id, c)
    }
  }
  const rows = items
    .filter((item) => matchesSearch(haystack(item.name, item.description), opts.search))
    .map((item) => ({ id: item.id, item, projects: 0, drifted: 0, ...counts.get(item.id) }))
  return rows.length > 0 ? [{ key: GROUP_NONE, label: opts.label, rows }] : []
}

export interface ProjectLink<P extends { id: string }> {
  project: P
  /** null = not applied to this project (or its state could not be read). */
  state: LinkState | null
}

/** Every project, in the given order, with the item's link state there. */
export function projectLinks<P extends { id: string }>(
  projects: readonly P[],
  applied: Readonly<Record<string, readonly AppliedLike[] | undefined>>,
  itemId: string,
): ProjectLink<P>[] {
  return projects.map((project) => ({
    project,
    state: applied[project.id]?.find((a) => a.item_id === itemId)?.state ?? null,
  }))
}

// ── Frontmatter ─────────────────────────────────────────────────────────────

/** The name the item is saved under: the agent's `name:`, or the command's own field. */
export function itemName(kind: LibraryKind, content: string, nameField: string): string {
  return kind === 'agent' ? parseFrontmatter(content).name : nameField.trim()
}

export function itemDescription(content: string): string {
  return parseFrontmatter(content).description
}

export type ItemError = 'errMissingName' | 'errNameSlug' | 'errMissingDescription'

// The backend's rule (`library_items::validate`): Rust `is_alphanumeric`, `-`, `_`.
const NAME_RE = /^[\p{L}\p{N}_-]+$/u

/** The first problem that would make the backend refuse the save (an i18n key), or null. */
export function validateItem(kind: LibraryKind, content: string, nameField: string): ItemError | null {
  const name = itemName(kind, content, nameField)
  if (!name) return 'errMissingName'
  if (!NAME_RE.test(name)) return 'errNameSlug'
  if (!itemDescription(content)) return 'errMissingDescription'
  return null
}

/**
 * {@link setFrontmatterField}, but a file without a frontmatter block gets one
 * (a command may start as plain text; its description still needs a home), and
 * an empty value drops the line: `parseFrontmatter` would read an empty `key:`
 * as the line after it.
 */
export function setItemField(raw: string, key: string, value: string | null): string {
  const v = value === '' ? null : value
  if (hasFrontmatter(raw)) return setFrontmatterField(raw, key, v)
  if (v === null) return raw
  return `---\n${key}: ${v}\n---\n${raw ? `\n${raw}` : ''}`
}

function hasFrontmatter(raw: string): boolean {
  return raw.startsWith('---') && raw.slice(3).includes('\n---')
}
