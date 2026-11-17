// Keys of the Tasks screen (V3-39): ↑ ↓ pick a todo (← → between the days of the
// week), Space done / undone, E edit, S snooze menu, Enter open. The host binds
// `onKeydown` to the frame that holds the rows; which key means what, and where
// the selection goes, is lib/taskKeys.ts. Nothing runs while a field has the
// key, a dialog or menu is open, or ⌘ / Ctrl / Alt is down.
import type { Ref } from 'vue'
import { useFocusActions } from '@/components/focus/useFocusActions'
import { hasOpenModal } from '@/lib/modalStack'
import { taskKeyAction, verticalSelection, weekNeighbour } from '@/lib/taskKeys'
import type { FocusTodo } from '@/lib/focus'

export interface TaskKeyboardOptions {
  /** Ids on screen, one list per column (one list when the view has no columns). */
  columns: () => string[][]
  week: () => boolean
  selectedId: Ref<string | null>
  todoOf: (id: string) => FocusTodo | null
  select: (id: string) => void
  open: (id: string) => void
  edit: (id: string) => void
  /** Absent: this host has no snooze menu, S does nothing. */
  snooze?: (id: string) => void
  /** After Space wrote (a host with its own copy of the todos reloads it). */
  changed?: () => void
}

export function useTaskKeyboard(o: TaskKeyboardOptions) {
  const actions = useFocusActions()

  async function toggleDone(id: string) {
    const todo = o.todoOf(id)
    if (!todo) return
    if (todo.done) await actions.undone(todo)
    else await actions.done(todo)
    o.changed?.()
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.defaultPrevented || hasOpenModal() || document.querySelector('.ui-popover')) return
    const action = taskKeyAction(e, { week: o.week() })
    if (!action) return

    const columns = o.columns()
    const ids = [...new Set(columns.flat())]
    const cur = o.selectedId.value !== null && ids.includes(o.selectedId.value) ? o.selectedId.value : null

    if (action === 'next' || action === 'prev' || action === 'left' || action === 'right') {
      const to =
        action === 'left' || action === 'right' ? weekNeighbour(columns, cur, action) : verticalSelection(columns, cur, action, o.week())
      if (to === null) return
      e.preventDefault()
      o.select(to)
      return
    }

    if (cur === null) return
    // A focused row already opens itself on Enter.
    if (action === 'open' && (e.target as HTMLElement | null)?.dataset?.focusRow !== undefined) return
    if (action === 'snooze' && !o.snooze) return
    e.preventDefault()
    if (action === 'done') void toggleDone(cur)
    else if (action === 'edit') o.edit(cur)
    else if (action === 'snooze') o.snooze?.(cur)
    else o.open(cur)
  }

  return { onKeydown }
}
