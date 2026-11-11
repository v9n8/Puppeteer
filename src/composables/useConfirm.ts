// Promise-based confirm dialog to replace the browser-native `confirm()`.
// A single <ConfirmModal> host (mounted in App.vue) reads this shared state and
// resolves the pending promise. Callers await the boolean, exactly like
// `confirm()`:
//
//   import { useConfirm } from '@/composables/useConfirm'
//   const { confirm } = useConfirm()
//   if (!(await confirm('Delete this?'))) return
//
// `choose()` adds a second action (`altLabel`) and resolves with which button
// was pressed, or null on cancel.
import { reactive } from 'vue'

export interface ConfirmOptions {
  /** Header text. */
  title?: string
  /** Body text; `\n` renders as line breaks. */
  message: string
  /** Confirm-button label. */
  confirmLabel?: string
  /** Cancel-button label. */
  cancelLabel?: string
  /** Confirm-button style. `destructive` (default) is red for delete actions. */
  variant?: 'destructive' | 'primary'
}

export interface ChoiceOptions extends ConfirmOptions {
  /** Label of the second action, shown between Cancel and Confirm. */
  altLabel: string
}

export type ConfirmChoice = 'confirm' | 'alt' | null

interface ConfirmState extends Required<ConfirmOptions> {
  open: boolean
  /** '' = no second action. */
  altLabel: string
}

const state = reactive<ConfirmState>({
  open: false,
  title: 'Confirm',
  message: '',
  confirmLabel: 'Confirm',
  cancelLabel: 'Cancel',
  variant: 'destructive',
  altLabel: '',
})

let resolver: ((value: ConfirmChoice) => void) | null = null

function openDialog(opts: ConfirmOptions, altLabel: string): Promise<ConfirmChoice> {
  // If a dialog is already pending, cancel it before opening the new one.
  resolver?.(null)

  state.title = opts.title ?? 'Confirm'
  state.message = opts.message
  state.confirmLabel = opts.confirmLabel ?? 'Confirm'
  state.cancelLabel = opts.cancelLabel ?? 'Cancel'
  state.variant = opts.variant ?? 'destructive'
  state.altLabel = altLabel
  state.open = true

  return new Promise<ConfirmChoice>((resolve) => {
    resolver = resolve
  })
}

function confirm(options: ConfirmOptions | string): Promise<boolean> {
  const opts = typeof options === 'string' ? { message: options } : options
  return openDialog(opts, '').then((value) => value === 'confirm')
}

function choose(options: ChoiceOptions): Promise<ConfirmChoice> {
  return openDialog(options, options.altLabel)
}

function respond(value: ConfirmChoice) {
  if (!state.open) return
  state.open = false
  resolver?.(value)
  resolver = null
}

export function useConfirm() {
  return { state, confirm, choose, respond }
}
