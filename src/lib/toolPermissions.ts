// A project's standing tool permissions ("Always allow" / "Always deny", UX-31):
// the shapes of the DB rows (`commands::tool_permissions`) and the one-time copy
// of the decisions older versions kept in localStorage. Kept free of Vue / Tauri
// / `@/` imports so it can be unit-tested with `node --test`.

export type ToolDecision = 'allow' | 'deny'

/** A row of `list_project_tool_permissions`. */
export interface ToolPermission {
  project_id: string
  tool_name: string
  decision: ToolDecision
  created_at: string
}

/** One decision for `import_project_tool_permissions`. */
export interface ToolPermissionImport {
  project_id: string
  tool_name: string
  decision: ToolDecision
}

export interface ProjectToolPerms {
  allow: string[]
  deny: string[]
}

/** Where versions before UX-31 kept the decisions: `{ [projectId]: { allow: [], deny: [] } }`. */
export const LEGACY_STORAGE_KEY = 'puppeteer.toolPermissions'
/**
 * Set once the legacy decisions are in the DB, so they are copied only once:
 * a decision revoked afterwards must not come back from the old copy. The old
 * copy itself is left as it was (an older build still reads it).
 */
export const LEGACY_IMPORTED_KEY = 'puppeteer.toolPermissions.importedToDb'

/**
 * The built-in `gdrive` + `gmail` MCP servers were merged into a single `google`
 * server, and their tools gained a group prefix (`search` → `drive_search`).
 * Stored decisions match by exact tool name, so an old name is rewritten on the
 * way in, or the user would be asked again for tools they already answered for.
 *
 * Per-SERVER keys (`mcp__gdrive`, emitted by codex runs, which can't name the
 * tool) are DROPPED rather than mapped to `mcp__google`: that would widen a
 * decision made about Drive alone into one covering Gmail and Calendar too.
 * Being asked once more is the safe failure.
 */
export function migrateToolName(name: string): string | null {
  if (name === 'mcp__gdrive' || name === 'mcp__gmail') return null
  if (name.startsWith('mcp__gdrive__')) return `mcp__google__drive_${name.slice('mcp__gdrive__'.length)}`
  if (name.startsWith('mcp__gmail__')) return `mcp__google__mail_${name.slice('mcp__gmail__'.length)}`
  return name
}

function toolNames(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  const out: string[] = []
  for (const v of value) {
    const name = typeof v === 'string' && v ? migrateToolName(v) : null
    if (name && !out.includes(name)) out.push(name)
  }
  return out
}

/**
 * The decisions in the legacy localStorage value, in their stored order. Anything
 * malformed (bad JSON, wrong shapes, non-string names) is skipped, never thrown.
 * A tool on both lists counts as denied, as the prompt check does.
 */
export function parseLegacyToolPermissions(raw: string | null): ToolPermissionImport[] {
  let obj: unknown
  try {
    obj = raw ? JSON.parse(raw) : null
  } catch {
    return []
  }
  if (!obj || typeof obj !== 'object' || Array.isArray(obj)) return []
  const out: ToolPermissionImport[] = []
  for (const [projectId, perms] of Object.entries(obj as Record<string, unknown>)) {
    if (!projectId || !perms || typeof perms !== 'object' || Array.isArray(perms)) continue
    const rec = perms as Record<string, unknown>
    const deny = toolNames(rec.deny)
    for (const tool of toolNames(rec.allow)) {
      if (!deny.includes(tool)) out.push({ project_id: projectId, tool_name: tool, decision: 'allow' })
    }
    for (const tool of deny) out.push({ project_id: projectId, tool_name: tool, decision: 'deny' })
  }
  return out
}

/** The storage calls the import makes (`localStorage` in the app). */
export interface KeyValueStorage {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
}

/**
 * Copy the legacy localStorage decisions into the DB once. `send` adds them
 * (the backend keeps any decision it already has, so a repeat is harmless).
 * The done mark is set only after `send` succeeded, so a failed copy is tried
 * again on the next start. Storage that throws counts as having nothing.
 */
export async function importLegacyToolPermissions(
  storage: KeyValueStorage | null,
  send: (entries: ToolPermissionImport[]) => Promise<unknown>,
): Promise<'imported' | 'nothing' | 'already' | 'failed'> {
  if (!storage) return 'nothing'
  let raw: string | null
  try {
    if (storage.getItem(LEGACY_IMPORTED_KEY)) return 'already'
    raw = storage.getItem(LEGACY_STORAGE_KEY)
  } catch {
    return 'nothing'
  }
  const entries = parseLegacyToolPermissions(raw)
  if (entries.length) {
    try {
      await send(entries)
    } catch {
      return 'failed'
    }
  }
  try {
    storage.setItem(LEGACY_IMPORTED_KEY, '1')
  } catch {
    /* storage unavailable: the next start sends the same entries again, which adds nothing */
  }
  return entries.length ? 'imported' : 'nothing'
}

/** Rows grouped per project, in their order (oldest decision first). */
export function groupToolPermissions(rows: readonly ToolPermission[]): Record<string, ProjectToolPerms> {
  const out: Record<string, ProjectToolPerms> = {}
  for (const r of rows) {
    const e = out[r.project_id] ?? (out[r.project_id] = { allow: [], deny: [] })
    if (r.decision === 'allow') e.allow.push(r.tool_name)
    else if (r.decision === 'deny') e.deny.push(r.tool_name)
  }
  return out
}
