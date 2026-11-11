// Import from other AI tools (backend `src-tauri/src/import`, `library.import_external`):
// the three local-only commands, their shapes (snake_case, as serde sends them), and
// the pure state of the Import dialog — which candidates are ticked, under what name,
// and what happens when the name is taken. Since UX-56 an import fills one of two
// places: the Library (rules, skills, agents, commands as files in `.claude/` of
// HOME or a project) or the MCP servers list.
//
// Kept free of Vue / Tauri / `@/` imports so it can be unit-tested with `node --test`.

export const IMPORT_FEATURE = 'library.import_external'
/** `{ into }` → every (adapter, scope) holding something to import there. */
export const CMD_LIST_IMPORT_SOURCES = 'list_import_sources'
/** `{ source, dest }` → `{ candidates, problems }`. */
export const CMD_SCAN_IMPORT_SOURCE = 'scan_import_source'
/** `{ source, dest, picks }` → one outcome per pick. */
export const CMD_IMPORT_CANDIDATES = 'import_candidates'

export type ImportKind = 'mcp' | 'rule' | 'skill' | 'agent' | 'command'
export type ImportScope = 'global' | 'project'
export type ImportInto = 'library' | 'mcp'
/** `overwrite`: files only — the one there goes to the Trash (a link is removed). */
export type OnConflict = 'skip' | 'rename' | 'overwrite'

/** Where an import goes: the Library's scope, or the MCP list. */
export type ImportDest =
  | { into: 'library'; scope: ImportScope; project_id: string | null }
  | { into: 'mcp' }

/** Section order on screen. */
export const KIND_ORDER: readonly ImportKind[] = ['mcp', 'rule', 'skill', 'agent', 'command']

/** What a taken name can be met with, per destination: an MCP server is never replaced. */
export const CONFLICT_CHOICES: Readonly<Record<ImportInto, readonly OnConflict[]>> = {
  library: ['rename', 'overwrite', 'skip'],
  mcp: ['rename', 'skip'],
}

export interface ImportSource {
  /** Adapter id (`claude_code`, …): the list is open, a new tool is one more id. */
  adapter: string
  scope: ImportScope
  project_id: string | null
  project_name: string | null
  root: string
  kinds: ImportKind[]
}

/** How the commands name a source: never by path. */
export interface ImportSourceRef {
  adapter: string
  scope: ImportScope
  project_id: string | null
}

export interface McpSummary {
  transport: string
  command: string | null
  args: string[]
  url: string | null
  /** KEY names only: a value never leaves the backend. */
  env_keys: string[]
  header_keys: string[]
}

export interface ImportCandidate {
  id: string
  kind: ImportKind
  /** Proposed name. */
  name: string
  description: string
  origin: string
  hash: string
  /** Where it goes, something of this kind is already named `name` (case ignored for files). */
  exists: boolean
  mcp: McpSummary | null
}

export interface ImportProblem {
  path: string
  item: string | null
  message: string
}

export interface ImportScan {
  candidates: ImportCandidate[]
  problems: ImportProblem[]
}

export interface ImportPick {
  id: string
  name?: string
  on_conflict: OnConflict
}

export type ImportStatus = 'imported' | 'skipped' | 'failed'

export interface ImportOutcome {
  id: string
  kind: ImportKind | null
  status: ImportStatus
  name: string | null
  message: string | null
}

// ── Sources ─────────────────────────────────────────────────────────────────

export function sourceKey(s: ImportSourceRef): string {
  return `${s.adapter}\n${s.scope}\n${s.project_id ?? ''}`
}

export function sourceRef(s: ImportSource): ImportSourceRef {
  return { adapter: s.adapter, scope: s.scope, project_id: s.project_id }
}

/** Whether `s` reads the place a Library import writes to: the pick offered first. */
export function readsDest(s: ImportSource, dest: ImportDest): boolean {
  return dest.into === 'library' && s.scope === dest.scope && (s.scope === 'global' || s.project_id === dest.project_id)
}

/** Adapter ids in the order the backend lists them. */
export function sourceAdapters(sources: readonly ImportSource[]): string[] {
  return [...new Set(sources.map((s) => s.adapter))]
}

// ── Selection ───────────────────────────────────────────────────────────────

export interface ImportChoice {
  picked: boolean
  name: string
  onConflict: OnConflict
}

export type ImportChoices = Record<string, ImportChoice>

/**
 * Fresh choices for a scan: new items ticked, names already there left unticked
 * (ticking one imports it under a free name unless told otherwise).
 */
export function initialChoices(candidates: readonly ImportCandidate[]): ImportChoices {
  const out: ImportChoices = {}
  for (const c of candidates) out[c.id] = { picked: !c.exists, name: c.name, onConflict: 'rename' }
  return out
}

export function withChoice(choices: ImportChoices, id: string, patch: Partial<ImportChoice>): ImportChoices {
  const current = choices[id]
  return current ? { ...choices, [id]: { ...current, ...patch } } : choices
}

/** Tick or untick every id in `ids`. */
export function withAllPicked(choices: ImportChoices, ids: readonly string[], picked: boolean): ImportChoices {
  const out = { ...choices }
  for (const id of ids) if (out[id]) out[id] = { ...out[id], picked }
  return out
}

// The backend's names (`import::library_name`, MCP's `validate_name`): ASCII
// letters, digits, `-`, `_` — a file name too.
const NAME_RE = /^[A-Za-z0-9_-]+$/

export type NameError = 'empty' | 'invalid'

export function nameError(name: string): NameError | null {
  if (!name) return 'empty'
  return NAME_RE.test(name) ? null : 'invalid'
}

/**
 * Why the name of a ticked candidate may be taken when it is imported: it is
 * already there (`exists`, known for the proposed name only), or an earlier
 * ticked candidate of the same kind uses it (`duplicate`: picks run in order,
 * the first one gets the name). `null` for an unticked candidate.
 */
export function conflictOf(
  candidates: readonly ImportCandidate[],
  choices: ImportChoices,
  id: string,
): 'exists' | 'duplicate' | null {
  const index = candidates.findIndex((c) => c.id === id)
  const cand = candidates[index]
  const choice = choices[id]
  if (!cand || !choice?.picked) return null
  if (cand.exists && choice.name === cand.name) return 'exists'
  const earlier = candidates
    .slice(0, index)
    .some((c) => c.kind === cand.kind && choices[c.id]?.picked && choices[c.id].name === choice.name)
  return earlier ? 'duplicate' : null
}

export interface CandidateSection {
  kind: ImportKind
  candidates: ImportCandidate[]
}

/** Candidates by kind in {@link KIND_ORDER}, each in scan order; empty kinds left out. */
export function candidateSections(candidates: readonly ImportCandidate[]): CandidateSection[] {
  return KIND_ORDER.map((kind) => ({ kind, candidates: candidates.filter((c) => c.kind === kind) })).filter(
    (s) => s.candidates.length > 0,
  )
}

/** The ticked candidates as picks, in scan order; a name only when it differs. */
export function buildPicks(candidates: readonly ImportCandidate[], choices: ImportChoices): ImportPick[] {
  const picks: ImportPick[] = []
  for (const c of candidates) {
    const choice = choices[c.id]
    if (!choice?.picked) continue
    const pick: ImportPick = { id: c.id, on_conflict: choice.onConflict }
    if (choice.name !== c.name) pick.name = choice.name
    picks.push(pick)
  }
  return picks
}

/** Something is ticked and every ticked name is valid. */
export function importReady(candidates: readonly ImportCandidate[], choices: ImportChoices): boolean {
  const picked = candidates.filter((c) => choices[c.id]?.picked)
  return picked.length > 0 && picked.every((c) => nameError(choices[c.id].name) === null)
}

// ── Outcomes ────────────────────────────────────────────────────────────────

// A type, not an interface: it is passed as the named values of an i18n message.
export type OutcomeCounts = Record<ImportStatus, number>

export function countOutcomes(outcomes: readonly ImportOutcome[]): OutcomeCounts {
  const counts: OutcomeCounts = { imported: 0, skipped: 0, failed: 0 }
  for (const o of outcomes) counts[o.status] += 1
  return counts
}
