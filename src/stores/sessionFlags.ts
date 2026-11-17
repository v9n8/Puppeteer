import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import type { ProjectColor } from '@/lib/projectAppearance'
import type { SessionFlag, SessionFlagList } from '@/lib/sessionFlags'
import { clearGoneFlags } from '@/lib/flagFilter'
import { useRunsStore } from '@/stores/runs'

/** Emitted by the backend when a flag definition changed (any window, a remote desktop too). */
const EVT_CHANGED = 'session-flags:changed'

/** The `byScope` key of the shared list (`list_session_flags` with no project). */
const SHARED = ''

/**
 * Session flags (V3-18), per scope: `byScope.get('')` is the shared flags as
 * Settings › Flag lists them (counts over every project); `byScope.get(projectId)`
 * is what that project's sessions see: the shared flags with the project's hidden
 * marks and counts, then its own. A scope is loaded by `load` and reloaded when the
 * backend says it changed; a change to the shared flags reloads every loaded scope.
 *
 * A session's flag lives on its run record: see `useRunsStore().setRunFlag`.
 */
export const useSessionFlagsStore = defineStore('sessionFlags', () => {
  const byScope = reactive(new Map<string, SessionFlag[]>())
  let listening: Promise<void> | null = null

  function key(projectId: string | null | undefined): string {
    return projectId ?? SHARED
  }

  /** The loaded flags of a scope (empty until `load`). */
  function flagsOf(projectId: string | null | undefined): SessionFlag[] {
    return byScope.get(key(projectId)) ?? []
  }

  async function load(projectId: string | null | undefined): Promise<SessionFlag[]> {
    void listenOnce()
    const list = await invoke<SessionFlagList>('list_session_flags', { projectId: projectId ?? null })
    byScope.set(key(projectId), list.flags)
    return list.flags
  }

  /** Reload what a change of `projectId`'s flags (null: the shared ones) touches. */
  async function reloadChanged(projectId: string | null) {
    // A project's own flags and hidden marks do not show in the shared list.
    const scopes = projectId === null ? [...byScope.keys()] : [projectId].filter((k) => byScope.has(k))
    await Promise.all(scopes.map((k) => load(k === SHARED ? null : k)))
    dropGoneFlagsFromRuns()
  }

  /** A flag deleted in another window leaves its id on the runs this window holds: clear it, so filters and counts agree. */
  function dropGoneFlagsFromRuns() {
    const runsStore = useRunsStore()
    const flagsOf = (projectId: string) => byScope.get(projectId) ?? null
    clearGoneFlags(runsStore.runs, flagsOf)
    clearGoneFlags(runsStore.runMeta.values(), flagsOf)
  }

  function listenOnce(): Promise<void> {
    listening ??= listen<{ project_id?: string | null }>(EVT_CHANGED, (event) => {
      reloadChanged(event.payload?.project_id ?? null).catch((e) => console.warn('[sessionFlags] reload failed', e))
    }).then(
      () => {},
      () => { /* no event bus (backend gone): this window's own writes still reload below */ },
    )
    return listening
  }

  // Each write reloads the scopes it touched in this window too, so the result
  // does not wait for the event (which every window, this one included, gets).

  async function create(projectId: string | null, name: string, color: ProjectColor): Promise<SessionFlag> {
    const flag = await invoke<SessionFlag>('create_session_flag', { projectId, name, color })
    await reloadChanged(projectId)
    return flag
  }

  async function update(flag: Pick<SessionFlag, 'id' | 'project_id'>, input: { name?: string; color?: ProjectColor }) {
    const next = await invoke<SessionFlag>('update_session_flag', { id: flag.id, name: input.name ?? null, color: input.color ?? null })
    await reloadChanged(flag.project_id)
    return next
  }

  /** Delete a flag; returns how many sessions lost it. */
  async function remove(flag: Pick<SessionFlag, 'id' | 'project_id'>): Promise<number> {
    const outcome = await invoke<{ cleared_runs: number }>('delete_session_flag', { id: flag.id })
    await reloadChanged(flag.project_id)
    return outcome.cleared_runs
  }

  /** Put the flags of a scope in the order of `ids` (every flag of the scope, once). */
  async function reorder(projectId: string | null, ids: string[]) {
    await invoke('reorder_session_flags', { projectId, ids })
    await reloadChanged(projectId)
  }

  /** Put back the deleted default flags; returns them. */
  async function restoreDefaults(): Promise<SessionFlag[]> {
    const restored = await invoke<SessionFlag[]>('restore_default_session_flags')
    if (restored.length) await reloadChanged(null)
    return restored
  }

  /** Hide a shared flag from a project's sessions (they keep it if they have it), or show it again. */
  async function setHidden(projectId: string, flagId: string, hidden: boolean) {
    await invoke('set_project_flag_hidden', { projectId, flagId, hidden })
    await reloadChanged(projectId)
  }

  return { byScope, flagsOf, load, create, update, remove, reorder, restoreDefaults, setHidden }
})
