// Import from other AI tools (backend `src-tauri/src/import`, `library.import_external`):
// the three local-only commands, their shapes (snake_case, as serde sends them), and
// the pure state of the Import screen — which candidates are ticked, under what name,
// for which engine, and what happens when the name is taken.
//
// Kept free of Vue / Tauri / `@/` imports so it can be unit-tested with `node --test`.

export const IMPORT_FEATURE = 'library.import_external'
/** → every (adapter, scope) holding something to import. */
export const CMD_LIST_IMPORT_SOURCES = 'list_import_sources'
/** `{ source }` → `{ candidates, problems }`. */
export const CMD_SCAN_IMPORT_SOURCE = 'scan_import_source'
/** `{ source, picks }` → one outcome per pick. */
export const CMD_IMPORT_CANDIDATES = 'import_candidates'

export type ImportKind = 'mcp' | 'rule' | 'skill' | 'agent' | 'command'
export type ImportScope = 'global' | 'project'
export type ImportTarget = 'claude' | 'codex' | 'both'
export type OnConflict = 'skip' | 'rename'

/** Section order on screen. */
export const KIND_ORDER: readonly ImportKind[] = ['mcp', 'rule', 'skill', 'agent', 'command']

/** Kinds whose library has a prop of its own; the others are core. */
export const KIND_FEATURE: Readonly<Partial<Record<ImportKind, string>>> = {
  agent: 'library.agents',
  command: 'library.commands',
}

export const TARGETS: readonly ImportTarget[] = ['claude', 'codex', 'both']

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
  /** Proposed library name. */
  name: string
  description: string
  origin: string
  /** Skills and rules only. */
  target: ImportTarget | null
  hash: string
  /** The library already has an item of this kind named `name`. */
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
  target?: ImportTarget
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

/**
 * The sources as the screen offers them: kinds whose library is off dropped (the
 * scan leaves them out too), and a source left with nothing dropped as well.
 */
export function visibleSources(sources: readonly ImportSource[], isOn: (feature: string) => boolean): ImportSource[] {
  return sources
    .map((s) => ({
      ...s,
      kinds: s.kinds.filter((k) => {
        const feature = KIND_FEATURE[k]
        return !feature || isOn(feature)
      }),
    }))
    .filter((s) => s.kinds.length > 0)
}

/** Adapter ids in the order the backend lists them. */
export function sourceAdapters(sources: readonly ImportSource[]): string[] {
  return [...new Set(sources.map((s) => s.adapter))]
}

// ── Selection ───────────────────────────────────────────────────────────────

export interface ImportChoice {
  picked: boolean
  name: string
  /** `null` for kinds without an engine choice. */
  target: ImportTarget | null
  onConflict: OnConflict
}

export type ImportChoices = Record<string, ImportChoice>

/** Skills and rules are written per engine; MCP servers, agents and commands are not chosen here. */
export function hasTarget(kind: ImportKind): boolean {
  return kind === 'skill' || kind === 'rule'
}

/**
 * Fresh choices for a scan: new items ticked, names the library already has left
 * unticked (ticking one imports it under a free name unless told to skip).
 */
export function initialChoices(candidates: readonly ImportCandidate[]): ImportChoices {
  const out: ImportChoices = {}
  for (const c of candidates) {
    out[c.id] = {
      picked: !c.exists,
      name: c.name,
      target: hasTarget(c.kind) ? (c.target ?? 'claude') : null,
      onConflict: 'rename',
    }
  }
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

// The backend's library names (`import::library_name`, MCP's `validate_name`):
// ASCII letters, digits, `-`, `_`.
const NAME_RE = /^[A-Za-z0-9_-]+$/

export type NameError = 'empty' | 'invalid'

export function nameError(name: string): NameError | null {
  if (!name) return 'empty'
  return NAME_RE.test(name) ? null : 'invalid'
}

/**
 * Why the name of a ticked candidate may be taken when it is imported: the
 * library has it (`exists`, known for the proposed name only), or an earlier
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

/** The ticked candidates as picks, in scan order; a name or target only when it differs. */
export function buildPicks(candidates: readonly ImportCandidate[], choices: ImportChoices): ImportPick[] {
  const picks: ImportPick[] = []
  for (const c of candidates) {
    const choice = choices[c.id]
    if (!choice?.picked) continue
    const pick: ImportPick = { id: c.id, on_conflict: choice.onConflict }
    if (choice.name !== c.name) pick.name = choice.name
    if (hasTarget(c.kind) && choice.target && choice.target !== (c.target ?? 'claude')) pick.target = choice.target
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

/** Kinds something was imported into: the libraries to list again. */
export function importedKinds(outcomes: readonly ImportOutcome[]): ImportKind[] {
  const kinds = new Set(outcomes.filter((o) => o.status === 'imported' && o.kind).map((o) => o.kind as ImportKind))
  return KIND_ORDER.filter((k) => kinds.has(k))
}
