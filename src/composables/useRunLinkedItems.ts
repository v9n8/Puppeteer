// What a session is linked to — Slack threads, notes, todos — loaded once for
// the info bar's Links menu (SessionLinksMenu): one request per source, not
// per menu. Resyncs on `slack-threads://changed` / `item:changed` (links,
// renames and deletes all emit them), debounced.
import { computed, onBeforeUnmount, onMounted, ref, watch, type Ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import { useToast } from '@/composables/useToast'
import { ITEM_CHANGED } from '@/lib/itemWindow'
import { listRunItems, type RunLinkedItem } from '@/lib/sessionLinks'
import {
  SLACK_THREADS_CHANGED,
  listRunSlackThreads,
  type SlackThreadSummary,
} from '@/stores/slackThreads'

export function useRunLinkedItems(runId: Ref<string | null>) {
  const { toast } = useToast()
  const threads = ref<SlackThreadSummary[]>([])
  const items = ref<RunLinkedItem[]>([])

  const notes = computed(() => items.value.filter((i) => i.kind === 'note'))
  const todos = computed(() => items.value.filter((i) => i.kind === 'todo'))

  let loadSeq = 0
  async function load() {
    const id = runId.value
    const seq = ++loadSeq
    if (!id) {
      threads.value = []
      items.value = []
      return
    }
    // Extras: a failing half keeps what is shown.
    const [th, it] = await Promise.all([
      listRunSlackThreads(id).catch(() => null),
      listRunItems(id).catch(() => null),
    ])
    if (seq !== loadSeq) return
    if (th) threads.value = th
    if (it) items.value = it
  }

  watch(runId, load, { immediate: true })

  let unlisten: UnlistenFn[] = []
  let timer: ReturnType<typeof setTimeout> | null = null

  function scheduleLoad() {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => void load(), 300)
  }

  onMounted(async () => {
    try {
      unlisten.push(await listen(SLACK_THREADS_CHANGED, scheduleLoad))
      unlisten.push(await listen(ITEM_CHANGED, scheduleLoad))
    } catch {
      /* running outside the Tauri shell */
    }
  })

  onBeforeUnmount(() => {
    if (timer) clearTimeout(timer)
    unlisten.forEach((off) => off())
    unlisten = []
  })

  /** Tick / untick a linked todo; the row flips at once and is reloaded if the call fails. */
  async function toggleTodo(id: string) {
    const todo = items.value.find((i) => i.kind === 'todo' && i.id === id)
    if (!todo) return
    todo.done = !todo.done
    try {
      await invoke('toggle_todo', { id })
    } catch (e) {
      toast.error(String(e))
      await load()
    }
  }

  return { threads, notes, todos, toggleTodo }
}
