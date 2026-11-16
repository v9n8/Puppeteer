// State of the turn status bar (V3-17): the last turn's status, steps and todo,
// taken from the entries RunLogPane shows (the on-disk log when one is open, else
// the live ones) and the run's live state. The decision is lib/turnStatus; this
// only feeds it, plus a 1 s clock that runs while a tool call is running.
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import {
  todoProgress, turnSteps, turnStatus, turnTodos, type TurnEntryLike,
} from '@/lib/turnStatus'
import { useRunSession } from './useRunSession'

export function useTurnStatus() {
  const {
    currentRunId, currentRun, currentStatus, session, isViewingHistory, historyEntries, liveEntries,
    historyLoading,
  } = useRunSession()

  const entries = computed<readonly TurnEntryLike[]>(
    () => (isViewingHistory.value ? historyEntries.value : liveEntries.value) as readonly TurnEntryLike[],
  )
  const running = computed(() => !isViewingHistory.value && currentStatus.value === 'running')
  const pendingRequest = computed(() => session.value?.permissionQueue[0] ?? null)

  const status = computed(() => turnStatus({
    entries: entries.value,
    running: running.value,
    loading: isViewingHistory.value && historyLoading.value,
    runStatus: currentStatus.value,
    liveStatus: running.value ? session.value?.liveStatus ?? null : null,
    pending: pendingRequest.value
      ? { toolName: pendingRequest.value.tool_name, toolInput: pendingRequest.value.tool_input }
      : null,
    endReason: session.value?.endReason ?? currentRun.value?.end_reason ?? null,
  }))
  const steps = computed(() => turnSteps(entries.value))
  const todos = computed(() => turnTodos(entries.value))
  const todo = computed(() => todoProgress(todos.value))
  /** When the last turn stopped, for "Stopped turn 3 at 10:52". */
  const stoppedAt = computed(() => currentRun.value?.finished_at ?? currentRun.value?.last_activity_at ?? null)

  // Ticks only while a tool call runs, so a bar in the background costs no CPU.
  const now = ref(Date.now())
  let timer: ReturnType<typeof setInterval> | null = null
  const stopClock = () => {
    if (timer) clearInterval(timer)
    timer = null
  }
  watch(
    () => status.value.kind === 'tool' || steps.value.some((s) => s.state === 'running'),
    (ticking) => {
      stopClock()
      if (!ticking) return
      now.value = Date.now()
      timer = setInterval(() => { now.value = Date.now() }, 1000)
    },
    { immediate: true },
  )
  onBeforeUnmount(stopClock)

  return { currentRunId, currentRun, status, steps, todos, todo, stoppedAt, now }
}
