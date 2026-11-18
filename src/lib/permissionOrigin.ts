/**
 * Who asked for a permission: the line under the prompt's title
 * ("Claude · opus-4-5 · main agent · 10:51 · 2 minutes ago"). Built only from
 * what the backend event really carries, so a broker prompt (no engine, model
 * or agent) never shows a stray "·" or a made-up "Claude".
 */
import { shortModelId } from './runDisplay.ts'
import { agoParts } from './remoteInbox.ts'

/** `PermissionRequestEvent.agent` (src-tauri/src/runs/permission.rs). */
export type PermissionAgent =
  | { kind: 'main' }
  | { kind: 'sub'; id?: string | null; name?: string | null }

export interface PermissionOrigin {
  engine?: string | null
  model?: string | null
  agent?: PermissionAgent | null
  /** RFC 3339 UTC. */
  requested_at?: string | null
}

export interface OriginLabels {
  justNow: string
  main: string
  sub: string
  subNamed: (name: string) => string
}

export interface OriginPart {
  key: 'engine' | 'model' | 'agent' | 'time' | 'ago'
  text: string
  /** Full value for a tooltip when `text` is shortened. */
  title?: string
}

// Product names, not translated.
const ENGINE_NAMES: Record<string, string> = { claude: 'Claude', codex: 'Codex' }

/** "Claude" / "Codex", or null for a missing or unknown engine. */
export function engineName(engine: string | null | undefined): string | null {
  return (engine && ENGINE_NAMES[engine]) || null
}

/** A string that names the engine when known, else the engine-less fallback. */
export function engineLabel(
  engine: string | null | undefined,
  withEngine: (name: string) => string,
  withoutEngine: string,
): string {
  const name = engineName(engine)
  return name ? withEngine(name) : withoutEngine
}

/** The parts of the origin line that the request really has; [] = draw no line. */
export function permissionOriginParts(
  req: PermissionOrigin,
  now: Date,
  locale: string,
  labels: OriginLabels,
): OriginPart[] {
  const parts: OriginPart[] = []
  const engine = engineName(req.engine)
  if (engine) parts.push({ key: 'engine', text: engine })
  if (req.model) parts.push({ key: 'model', text: shortModelId(req.model), title: req.model })
  const agent = req.agent
  if (agent?.kind === 'main') parts.push({ key: 'agent', text: labels.main })
  else if (agent?.kind === 'sub') {
    parts.push({ key: 'agent', text: agent.name ? labels.subNamed(agent.name) : labels.sub, title: agent.id ?? undefined })
  }
  const at = req.requested_at ? Date.parse(req.requested_at) : NaN
  if (!Number.isNaN(at)) {
    const time = new Date(at).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' })
    parts.push({ key: 'time', text: time })
    // Minute steps only: the caller's clock ticks once a minute (useMinuteClock).
    const { value, unit } = agoParts(at, now.getTime())
    const ago = unit === 'second' ? labels.justNow : new Intl.RelativeTimeFormat(locale).format(value, unit)
    parts.push({ key: 'ago', text: ago })
  }
  return parts
}
