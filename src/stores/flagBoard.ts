import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import type { SessionFlag } from '@/lib/sessionFlags'
import type { HiddenMark } from '@/lib/flagBoard'
import { useRunsStore, type RunRecord } from '@/stores/runs'

/** A Session as the board lists it: its record, plus its Project's name and its workers. */
export interface BoardRun extends RunRecord {
  project_name: string
  worker_count: number
}

interface BoardData {
  runs: BoardRun[]
  flags: SessionFlag[]
  hidden: HiddenMark[]
}

const EVT_FLAGS_CHANGED = 'session-flags:changed'

/**
 * The flag board's data (V3-22): the Sessions and flags of one Project (`scope` =
 * its id) or of all of them (`scope` = null), loaded by `list_board_runs`. While the
 * screen is open (`start` … `stop`) a change of the flag definitions reloads it.
 * Dropping a card is `set_run_flag`; the card moves at once and goes back if that fails.
 */
export const useFlagBoardStore = defineStore('flagBoard', () => {
  const data = ref<BoardData | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  /** Cards whose flag is being saved (they show a spinner). */
  const saving = ref(new Set<string>())
  let scope: string | null = null
  let loaded = false
  let started = false
  let unlisten: (() => void) | null = null
  let ticket = 0

  /** Load a scope; a slower answer to an older call is dropped. */
  async function load(projectId: string | null) {
    // Another Project's columns must not stay on screen while this one loads.
    if (loaded && scope !== projectId) data.value = null
    scope = projectId
    const mine = ++ticket
    loading.value = true
    error.value = null
    try {
      const next = await invoke<BoardData>('list_board_runs', { projectId })
      if (mine === ticket) {
        data.value = next
        loaded = true
      }
    } catch (e) {
      if (mine === ticket) error.value = String(e)
    } finally {
      if (mine === ticket) loading.value = false
    }
  }

  function reload() {
    return load(scope)
  }

  async function start() {
    if (started) return
    started = true
    const off = await listen(EVT_FLAGS_CHANGED, () => {
      if (started) void reload()
    }).catch(() => null) // no event bus (backend gone): the buttons still reload by hand
    if (started) unlisten = off
    else off?.()
  }

  function stop() {
    started = false
    unlisten?.()
    unlisten = null
  }

  /** Give `run` the flag (null: none). Resolves to its flag before; throws, card back, if the save fails. */
  async function setFlag(run: BoardRun, flagId: string | null): Promise<string | null> {
    const before = run.flag_id ?? null
    run.flag_id = flagId
    saving.value = new Set(saving.value).add(run.id)
    try {
      await useRunsStore().setRunFlag(run.id, flagId)
      return before
    } catch (e) {
      run.flag_id = before
      throw e
    } finally {
      const next = new Set(saving.value)
      next.delete(run.id)
      saving.value = next
    }
  }

  return { data, loading, error, saving, load, reload, start, stop, setFlag }
})
