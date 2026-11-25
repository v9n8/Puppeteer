// Dropping dragged board cards on a column: one card, or every marked card at once.
// The cards are split for the column (lib/flagBoard planGroupDrop): those that may go
// there are saved side by side, one `set_run_flag` each; refused ones are never saved,
// so they stay in their column (a group says which and why); cards already there are
// skipped. One Undo puts each saved card back on its own old flag. While a group is
// dragged, the same split for the column under the pointer drives the column's look.
import { computed, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useFlagBoardStore, type BoardRun } from '@/stores/flagBoard'
import { useToast } from '@/composables/useToast'
import { cardTitle, dropFlagId, planGroupDrop, type BoardColumn, type HiddenMark } from '@/lib/flagBoard'
import type { BoardDragState } from './useBoardDrag'

type Moved = { run: BoardRun; before: string | null }

export function useBoardGroupDrop(options: {
  columns: Readonly<Ref<BoardColumn[]>>
  hidden: Readonly<Ref<HiddenMark[]>>
  /** The mouse drag in progress; a getter, since useBoardDrag is set up after this. */
  drag: () => BoardDragState | null
  columnLabel: (c: BoardColumn) => string
  projectNameOf: (id: string) => string
}) {
  const { t } = useI18n()
  const { toast } = useToast()
  const board = useFlagBoardStore()
  const { columnLabel } = options

  /** How the dragged cards split for the column under the pointer. */
  const overPlan = computed(() => {
    const d = options.drag()
    const column = d?.over ? options.columns.value.find((c) => c.id === d.over) : undefined
    return d && column ? planGroupDrop(d.runs, column, options.hidden.value) : null
  })

  /** Why `run` may not land on `c`: another Project's own flag, or a shared flag its Project hides. */
  function blockReasonFor(run: BoardRun, c: BoardColumn): string {
    const owner = c.flag?.project_id
    if (owner) return t('board.column.ownOther', { project: options.projectNameOf(owner) })
    return t('board.column.blocked', { project: run.project_name })
  }

  /** The first three titles, quoted, and an ellipsis when there are more. */
  function blockedNames(runs: BoardRun[]): string {
    const names = runs.slice(0, 3).map((r) => `“${cardTitle(r, t)}”`).join(', ')
    return runs.length > 3 ? `${names}, …` : names
  }

  async function dropGroup(runs: BoardRun[], columnId: string) {
    const column = options.columns.value.find((c) => c.id === columnId)
    if (!column) return
    const plan = planGroupDrop(runs, column, options.hidden.value)
    // A lone card shows why on the column while it hovers; a group names its refused cards here.
    if (runs.length > 1 && plan.blocked.length) {
      toast.error(t('board.toast.blockedMany', {
        n: plan.blocked.length,
        column: columnLabel(column),
        names: blockedNames(plan.blocked),
        reason: blockReasonFor(plan.blocked[0], column),
      }), 6000)
    }
    if (!plan.move.length) return
    const flagId = dropFlagId(column)
    // A failed save puts its own card back (stores/flagBoard); the others keep their new flag.
    const results = await Promise.allSettled(plan.move.map((run) => board.setFlag(run, flagId)))
    const moved: Moved[] = []
    const errors: unknown[] = []
    results.forEach((r, i) => {
      if (r.status === 'fulfilled') moved.push({ run: plan.move[i], before: r.value })
      else errors.push(r.reason)
    })
    if (errors.length === 1) toast.error(String(errors[0]))
    else if (errors.length > 1) toast.error(t('board.toast.failedMany', { n: errors.length, error: String(errors[0]) }))
    // The success toast tells only what was saved.
    if (moved.length === 1) {
      const { run, before } = moved[0]
      const name = cardTitle(run, t)
      const message = column.flag
        ? t('board.toast.set', { flag: columnLabel(column), name })
        : t('board.toast.removed', { name })
      toast.success(message, 6000, { label: t('board.toast.undo'), run: () => void undo(run, before, name) })
    } else if (moved.length > 1) {
      const message = column.flag
        ? t('board.toast.setMany', { flag: columnLabel(column), n: moved.length })
        : t('board.toast.removedMany', { n: moved.length })
      toast.success(message, 6000, { label: t('board.toast.undo'), run: () => void undoGroup(moved) })
    }
  }

  async function undo(run: BoardRun, before: string | null, name: string) {
    try {
      await board.setFlag(run, before)
      toast.success(t('board.toast.undone', { name }))
    } catch (e) {
      toast.error(String(e))
    }
  }

  /** Each card back on its own flag from before the drop. */
  async function undoGroup(moved: Moved[]) {
    const results = await Promise.allSettled(moved.map(({ run, before }) => board.setFlag(run, before)))
    const back = moved.filter((_, i) => results[i].status === 'fulfilled')
    if (back.length === 1) toast.success(t('board.toast.undone', { name: cardTitle(back[0].run, t) }))
    else if (back.length > 1) toast.success(t('board.toast.undoneMany', { n: back.length }))
    const failed = results.find((r): r is PromiseRejectedResult => r.status === 'rejected')
    if (failed) toast.error(String(failed.reason))
  }

  return { overPlan, blockReasonFor, dropGroup }
}
