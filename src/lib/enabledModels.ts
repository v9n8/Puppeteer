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
