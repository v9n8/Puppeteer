// The files a Session changed, for the "Changed files" tab (V3-26) and the number
// on its tab. One scan at a time for the Session on screen, shared by whoever
// reads it: the tab bar reads the count, the tab itself keeps it up to date.
//
// The scan is the whole log (`get_run_edit_records`), not the window the viewer
// loaded. The running Session's own entries are merged over it, since the log
// reaches the disk in batches.
import { computed, onScopeDispose, ref, shallowRef, watch } from 'vue'
import { changedFiles, isWriteTool, mergeLiveEdits, type ChangedFile } from '@/lib/editStats'
import { parseStreamLogWindow, type StreamEntry } from '@/lib/streamEvents'
import { useRunsStore } from '@/stores/runs'
import { useRunSession } from './useRunSession'

const RESCAN_DELAY_MS = 1500

// What the last successful scan of `scannedRun` returned; kept while a rescan runs
// or fails, so the list does not blink away.
const scannedRun = ref<string | null>(null)
const scanned = shallowRef<StreamEntry[]>([])
const tooLarge = shallowRef<ReadonlySet<string>>(new Set())
const loading = ref(false)
const error = ref<string | null>(null)
let scanSeq = 0

async function scan(runId: string, load: (id: string) => Promise<{ records: string[]; truncated: string[] }>) {
  const seq = ++scanSeq
  loading.value = true
  try {
    const r = await load(runId)
    if (seq !== scanSeq) return
    scanned.value = parseStreamLogWindow(r.records).entries
    tooLarge.value = new Set(r.truncated)
    scannedRun.value = runId
    error.value = null
  } catch (e) {
    if (seq !== scanSeq) return
    error.value = String(e)
  } finally {
    if (seq === scanSeq) loading.value = false
  }
}

/** The tab bar's side: how many files the shown Session changed, once the tab has scanned it. */
export function useChangedFilesCount() {
  const { currentRunId, liveEntries } = useRunSession()
  return computed(() => (
    scannedRun.value === currentRunId.value
      ? changedFiles(mergeLiveEdits(scanned.value, liveEntries.value), tooLarge.value).length
      : null
  ))
}

/** The tab's side: scans on open, again on "Reload" and when the Session's edits change. */
export function useChangedFiles() {
  const runsStore = useRunsStore()
  const { currentRunId, liveEntries } = useRunSession()

  const live = computed(() => (scannedRun.value === currentRunId.value ? liveEntries.value : []))
  const files = computed<ChangedFile[]>(() => {
    if (scannedRun.value !== currentRunId.value) return []
    return changedFiles(mergeLiveEdits(scanned.value, live.value), tooLarge.value)
  })

  function reload() {
    const id = currentRunId.value
    if (!id) return Promise.resolve()
    // Another Session's list must not show while this one scans.
    if (scannedRun.value !== id) {
      scannedRun.value = null
      scanned.value = []
    }
    return scan(id, (rid) => runsStore.getRunEditRecords(rid))
  }

  watch(currentRunId, () => { void reload() }, { immediate: true })

  // A finished edit call: rescan, so the list is right once the live entries go.
  const finishedEdits = computed(() => {
    let n = 0
    for (const e of liveEntries.value) if (e.kind === 'tool' && isWriteTool(e.name) && e.result) n++
    return n
  })
  let timer: ReturnType<typeof setTimeout> | null = null
  watch(finishedEdits, () => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => { void reload() }, RESCAN_DELAY_MS)
  })
  onScopeDispose(() => { if (timer) clearTimeout(timer) })

  return { files, loading, error, reload }
}
