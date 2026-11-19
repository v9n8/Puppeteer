// The board's column filter for the scope on screen (UR-60): the saved choice
// brought up to date with today's flags, the columns it gives, and `apply` to save
// a new one. Saved per machine in localStorage, one entry per scope (All projects,
// or a Project id); the rules live in lib/boardFilter.
import { computed, ref, type Ref } from 'vue'
import type { SessionFlag } from '@/lib/sessionFlags'
import {
  filterColumns, isDefaultFilter, parseFilter, reconcileFilter, serializeFilter, BOARD_FILTER_KEY,
  type BoardFilter, type SavedFilters,
} from '@/lib/boardFilter'

function loadSaved(): SavedFilters {
  try {
    return parseFilter(localStorage.getItem(BOARD_FILTER_KEY))
  } catch {
    return {}
  }
}

export function useBoardFilter(options: {
  /** 'all' or a Project id. */
  scope: Readonly<Ref<string>>
  flags: Readonly<Ref<SessionFlag[]>>
  /** The Project list order (groups of a new scope follow it). */
  projectOrder: Readonly<Ref<string[]>>
}) {
  const { scope, flags, projectOrder } = options
  const saved = ref<SavedFilters>(loadSaved())

  const filter = computed(() => reconcileFilter(saved.value[scope.value], flags.value, scope.value, projectOrder.value))
  const columns = computed(() => filterColumns(filter.value, flags.value, scope.value))
  const isDefault = computed(() => isDefaultFilter(filter.value, flags.value, scope.value, projectOrder.value))

  function apply(next: BoardFilter) {
    saved.value = { ...saved.value, [scope.value]: next }
    try {
      localStorage.setItem(BOARD_FILTER_KEY, serializeFilter(saved.value))
    } catch {
      /* storage unavailable — the in-memory choice still works for this session */
    }
  }

  return { filter, columns, isDefault, apply }
}
