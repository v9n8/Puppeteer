// History's flag side (V3-19): the project's flags, the "Filter by flag" choice and
// the flag shown on a row. Part of useRunHistoryList: it hands over the runs that
// passed the search, and gets back those that also pass the flag filter.
import { computed, reactive, watch, type ComputedRef } from 'vue'
import type { RunRecord } from '@/stores/runs'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import {
  filterByFlag, filterOptions, flagCounts, resetIfGone, type FlagFilter,
} from '@/lib/flagFilter'
import { flagById, flagLabel } from '@/lib/sessionFlags'

/** The filter of each project, kept while the app is open: coming back to a project finds it as left. */
const filterByProject = reactive(new Map<string, FlagFilter>())

export function useRunFlagFilter(
  projectId: ComputedRef<string>,
  searchedRuns: ComputedRef<RunRecord[]>,
) {
  const flagsStore = useSessionFlagsStore()

  const projectFlags = computed(() => flagsStore.flagsOf(projectId.value))
  watch(
    projectId,
    (id) => { flagsStore.load(id).catch((e) => console.warn('[sessionFlags] load failed', e)) },
    { immediate: true },
  )

  const flagFilter = computed<FlagFilter>({
    get: () => filterByProject.get(projectId.value) ?? 'all',
    set: (value) => { filterByProject.set(projectId.value, value) },
  })

  // A flag deleted in Settings (or another window) while it is the filter: back to "All flags".
  watch([projectFlags, flagFilter], () => {
    if (!flagsStore.byScope.has(projectId.value)) return
    flagFilter.value = resetIfGone(flagFilter.value, projectFlags.value)
  })

  const counts = computed(() => flagCounts(searchedRuns.value))
  const flagOptions = computed(() => filterOptions(projectFlags.value, counts.value))
  const flagFilteredRuns = computed(() => filterByFlag(searchedRuns.value, flagFilter.value))

  /** The flag a row shows: colour and name; null for a session without one. */
  function runFlag(run: RunRecord) {
    const flag = flagById(projectFlags.value, run.flag_id)
    return flag ? { color: flag.color, label: flagLabel(flag) } : null
  }

  return { projectFlags, flagFilter, flagCounts: counts, flagOptions, flagFilteredRuns, runFlag }
}
