// Memory: the instruction files the CLIs read on their own. The global ones
// (`~/.claude/CLAUDE.md`, Codex's `~/.codex/AGENTS.md`) are edited in
// Settings › AI › Memory, a project's CLAUDE.md / AGENTS.md in Library › By
// project (D12). Pure helpers of MemorySettings.vue and the Library; the paths
// are resolved by the backend (commands/memory.rs).

export const CMD_MEMORY_READ = 'memory_read'
export const CMD_MEMORY_WRITE = 'memory_write'

/** Settings › AI › Memory: one global file per CLI (Claude, Codex). */
export const GLOBAL_MEMORY_KINDS = ['user', 'codex_user'] as const
/** Library › By project › Memory: one file per CLI in the project's root. */
export const PROJECT_MEMORY_KINDS = ['project_claude', 'project_agents'] as const

export type GlobalMemoryKind = (typeof GLOBAL_MEMORY_KINDS)[number]
export type ProjectMemoryKind = (typeof PROJECT_MEMORY_KINDS)[number]
export type MemoryKind = GlobalMemoryKind | ProjectMemoryKind

/** What `memory_read` / `memory_write` take: a kind, never a path. */
export type MemoryTarget = { kind: GlobalMemoryKind } | { kind: ProjectMemoryKind; project_id: string }

/** What `memory_read` / `memory_write` return. */
export interface MemoryFile {
  path: string
  /** Where it really is, when `path` is a link. */
  link_target: string | null
  exists: boolean
  content: string
  size: number
  /** Passed back on save; null when the file didn't exist. */
  hash: string | null
  /** `kind:name` of the blocks Rules / Skills keep in this file. */
  managed_blocks: string[]
}

/** Error prefix of a save refused because the file changed on disk. */
export const MEMORY_CONFLICT = 'memory_conflict'

/** The target to read, or null while a project kind has no project picked. */
export function memoryTarget(kind: MemoryKind, projectId: string | null): MemoryTarget | null {
  if (kind === 'user' || kind === 'codex_user') return { kind }
  return projectId ? { kind, project_id: projectId } : null
}

/** Rough token count of `text` (≈ 4 characters a token), for the "≈" label. */
export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4)
}

/** Size of `text` on disk, as UTF-8. */
export function utf8Bytes(text: string): number {
  return new TextEncoder().encode(text).length
}

export function isMemoryConflict(err: unknown): boolean {
  return String(err).startsWith(MEMORY_CONFLICT)
}
