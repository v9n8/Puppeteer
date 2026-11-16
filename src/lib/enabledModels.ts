// The models that are on per engine (Settings › Model, V3-62). Same rules as the
// backend's `model_policy::EnabledModels`: the setting is
// `{"claude": [id…] | null, "codex": [id…] | null}`, and `null`, a missing engine,
// a missing setting or broken JSON all mean "every model of that engine is on".
// `[]` leaves only "Default (engine picks)". Kept free of Vue / Tauri so the
// remote controller and the node tests import it as is.

export interface EnabledModels {
  claude: string[] | null
  codex: string[] | null
}

export const ALL_MODELS_ON: EnabledModels = { claude: null, codex: null }

function idList(value: unknown): string[] | null {
  if (!Array.isArray(value)) return null
  const out: string[] = []
  for (const item of value) {
    if (typeof item !== 'string') continue
    const id = item.trim()
    if (id && !out.includes(id)) out.push(id)
  }
  return out
}

/** Read the stored JSON string, or the already-parsed object a phone frame carries. */
export function parseEnabledModels(raw: unknown): EnabledModels {
  let value = raw
  if (typeof raw === 'string') {
    try {
      value = JSON.parse(raw)
    } catch {
      return { ...ALL_MODELS_ON }
    }
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) return { ...ALL_MODELS_ON }
  const o = value as Record<string, unknown>
  return { claude: idList(o.claude), codex: idList(o.codex) }
}

/** A start the backend refused because its model is off (`run.errors.*`). */
export interface ModelBlockedError {
  code: 'modelDisabled' | 'modelDefaultDisabled'
  model: string
}

const BLOCKED_CODES: Record<string, ModelBlockedError['code']> = {
  model_disabled: 'modelDisabled',
  model_default_disabled: 'modelDefaultDisabled',
}

/** The backend's `"model_disabled:<id>: …"` / `"model_default_disabled:<id>: …"`, else null. */
export function modelBlockedError(e: unknown): ModelBlockedError | null {
  const msg = String(e instanceof Error ? e.message : e)
  const m = /^(model_disabled|model_default_disabled):(\S+?): /.exec(msg)
  return m ? { code: BLOCKED_CODES[m[1]], model: m[2] } : null
}

// Settings › Model (V3-63) -----------------------------------------------------

/** The ids the list decides about, like the backend's `model_policy::governed`:
 * '' (the engine picks) and a Claude family alias (`sonnet`) never are. */
function governed(engine: string, id: string): boolean {
  if (!id) return false
  if (engine === 'codex') return true
  return engine === 'claude' && id.toLowerCase().startsWith('claude-')
}

/** Whether `id` names a model of `engine` the list turns off. `null` (or an
 * engine with no list) = nothing is off. A fallback list (`a, b`) is off when
 * any of its ids is. */
export function isModelOff(engine: string, id: string | null | undefined, enabled: EnabledModels | null): boolean {
  const list = engine === 'claude' || engine === 'codex' ? enabled?.[engine] : null
  if (!list) return false
  return (id ?? '')
    .split(',')
    .map((m) => m.trim())
    .some((m) => governed(engine, m) && !list.includes(m))
}

/** Whether `id` is on in a saved engine list (`null` = all on). */
export function isOnIn(list: string[] | null, id: string): boolean {
  return list === null || list.includes(id)
}

/** One model row of Settings › Model. `usable: false` = the last scan found the
 * account cannot use it (`reason` = the scan's error). */
export interface ModelRow {
  id: string
  on: boolean
  usable: boolean
  reason?: string
}

export interface ModelVerdict {
  ok: boolean
  error?: string | null
}

/**
 * The "Available" / "Not available" blocks of an engine tab, from the SAVED
 * list (rows changed but not saved stay in their block). Available = on and not
 * found unusable by the last scan. Both keep `ids`' order.
 */
export function modelBlocks(
  ids: string[],
  list: string[] | null,
  verdicts: Record<string, ModelVerdict>,
): { available: ModelRow[]; unavailable: ModelRow[] } {
  const available: ModelRow[] = []
  const unavailable: ModelRow[] = []
  for (const id of ids) {
    const v = verdicts[id]
    const usable = !v || v.ok
    const row: ModelRow = { id, on: isOnIn(list, id), usable }
    if (!usable && v?.error) row.reason = v.error
    ;(usable && row.on ? available : unavailable).push(row)
  }
  return { available, unavailable }
}

/** A row's switch: the saved state, flipped while the row has an unsaved change. */
export function draftOn(list: string[] | null, flipped: ReadonlySet<string>, id: string): boolean {
  return isOnIn(list, id) !== flipped.has(id)
}

/**
 * The engine list to save. Nothing flipped keeps the saved value (`null` stays
 * "all on", so a model Codex lists later is on too). Otherwise the ids that are
 * on, in display order (the first one is where a moved default goes), then any
 * saved id no longer listed, so it is not dropped silently.
 */
export function engineListToSave(ids: string[], list: string[] | null, flipped: ReadonlySet<string>): string[] | null {
  if (!flipped.size) return list
  const on = ids.filter((id) => draftOn(list, flipped, id))
  const extra = (list ?? []).filter((id) => !ids.includes(id))
  return [...on, ...extra]
}

/** "Turn on a · turn off b" of the Save dialog, in display order. */
export function draftSummary(
  ids: string[],
  list: string[] | null,
  flipped: ReadonlySet<string>,
): { on: string[]; off: string[] } {
  const changed = ids.filter((id) => flipped.has(id))
  return {
    on: changed.filter((id) => !isOnIn(list, id)),
    off: changed.filter((id) => isOnIn(list, id)),
  }
}
