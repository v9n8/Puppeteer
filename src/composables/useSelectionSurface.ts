// Declare a "selection surface": an area whose selected text gets the shared
// selection toolbar (components/SelectionToolbar.vue, mounted once per window by
// App.vue). A screen only says which elements, which buttons, and where a quote
// / a capture goes; detecting the selection, placing the bar and running the
// actions are the toolbar's job. The rules themselves live in lib/selectionSurface.
//
// The registry is module state: each webview has its own copy, so a surface only
// ever meets the toolbar of its own window.
import { onBeforeUnmount, onDeactivated, shallowRef, type Ref } from 'vue'
import type { ItemContext } from '@/lib/itemWindow'
import type { SelectionAllow } from '@/lib/selectionSurface'
import type { TurnSide } from '@/lib/turnActions'
import type { QuoteAnchor } from '@/lib/quoteAnchor'
import type { SentMessage } from '@/lib/editResend'

/** Where a quote goes; `label` names it on the button. */
export interface QuoteTarget {
  label: string
  /**
   * `anchor`: where in the session's log the text was taken from (UX-29), if
   * known. `note`: the user's remark on it, typed in the Quote popover (UX-46).
   */
  append: (text: string, anchor?: QuoteAnchor | null, note?: string) => void
}

/** Taking the session shown on a surface back to one of its turns (Rewind). */
export interface TurnRewind {
  /** Turn `turn`'s snapshot can be restored now. */
  available: (turn: number) => boolean
  /** Ask (confirm first) to rewind to just before `turn`. */
  start: (turn: number) => void
}

/** Editing and resending a message of the session shown on a surface (UX-30). */
export interface TurnEditResend {
  /** The message sent as turn `turn` can be edited now (Send rewinds to it). */
  available: (turn: number) => boolean
  /** Put the message (entry `promptAt` of the shown log) into the composer to edit. */
  start: (turn: number, promptAt: number, sent: SentMessage) => void
}

/** Forking the session shown on a surface after one of its turns (UX-28). */
export interface TurnFork {
  /** Ask (confirm first) to fork after the turn starting at entry `promptAt` (-1 = before the loaded window). */
  start: (promptAt: number) => void
}

/** Bookmarks on the turns of the session shown on a surface. */
export interface TurnBookmarks {
  /**
   * Whether one side of the turn starting at entry `promptAt` of the shown log
   * is bookmarked (-1 = the turn before the loaded window); null = that turn
   * cannot be bookmarked.
   */
  marked: (promptAt: number, side: TurnSide) => boolean | null
  toggle: (promptAt: number, side: TurnSide) => void
}

/** What "save as note / todo" files the capture under. */
export interface SelectionCapture {
  context: ItemContext
  noteTitle?: string
}

export interface SelectionSurface {
  /** The element(s) whose selected text counts. */
  el: Ref<HTMLElement | null> | Ref<HTMLElement | null>[]
  allow: SelectionAllow
  /** Raw markdown the rendered text came from, tried in order (Copy MD). */
  markdownSource?: () => readonly string[]
  /** Null = no target now (Quote hidden). */
  quoteTarget?: () => QuoteTarget | null
  capture?: () => SelectionCapture
  /** "Rewind to here" in the turn rows of a log shown here; null = not now. */
  rewind?: () => TurnRewind | null
  /** "Edit and resend" in the turn rows of a log shown here; null = not now. */
  editResend?: () => TurnEditResend | null
  /** "Fork from here" in the turn rows of a log shown here; null = not now. */
  fork?: () => TurnFork | null
  /** Bookmark in the turn rows of a log shown here; null = not now. */
  bookmarks?: () => TurnBookmarks | null
}

/** A selection the bar is shown for. */
export interface SelectionBar {
  surface: SelectionSurface
  text: string
  /** The selection itself, for Quote to anchor on (lib/quoteAnchor). */
  range: Range | null
  rect: { left: number; top: number; bottom: number }
  /** Where the selection was right-clicked: the bar sits there, not under `rect`. */
  point: { x: number; y: number } | null
  quoteTarget: QuoteTarget | null
}

/** A translation popover opened from the bar. */
export interface SelectionTranslation {
  surface: SelectionSurface
  text: string
  x: number
  y: number
}

/** A quote waiting for its note in the Quote popover (UX-46), opened from the bar or a turn's row. */
export interface SelectionQuoteNote {
  surface: SelectionSurface
  /** Taken when Quote was clicked: a file window's target names the lines selected then. */
  target: QuoteTarget
  text: string
  anchor: QuoteAnchor | null
  /** What Quote was clicked from (the bar's spot, a turn's button), in viewport coordinates. */
  at: { left: number; top: number; bottom: number }
}

/**
 * What this window does with a surface's text besides the surface's own
 * callbacks: read the feature tree, file a note. Set by the toolbar; null where
 * there is none — the controller web client renders StreamLog without Pinia or
 * Tauri. The row under each chat turn (components/sessions/MsgActions.vue)
 * acts through it, so its Quote / note go where the selection bar's would.
 */
export interface SelectionHost {
  isOn: (featureId: string) => boolean
  saveNote: (surface: SelectionSurface, text: string) => void
}

/** Surfaces declared in this window, in mount order. */
export const selectionSurfaces: SelectionSurface[] = []
export const selectionHost = shallowRef<SelectionHost | null>(null)
export const selectionBar = shallowRef<SelectionBar | null>(null)
export const selectionTranslation = shallowRef<SelectionTranslation | null>(null)
export const selectionQuoteNote = shallowRef<SelectionQuoteNote | null>(null)

export function surfaceEls(surface: SelectionSurface): (HTMLElement | null)[] {
  return (Array.isArray(surface.el) ? surface.el : [surface.el]).map((r) => r.value)
}

/**
 * Register `surface` for the calling component's lifetime. `dismiss` hides the
 * bar and closes the translation and the Quote popover if they belong to it —
 * for when its content is replaced (another run, another file).
 */
export function useSelectionSurface(surface: SelectionSurface): { dismiss: () => void } {
  selectionSurfaces.push(surface)

  function dismiss() {
    if (selectionBar.value?.surface === surface) selectionBar.value = null
    if (selectionTranslation.value?.surface === surface) selectionTranslation.value = null
    if (selectionQuoteNote.value?.surface === surface) selectionQuoteNote.value = null
  }

  // A cached (KeepAlive) view keeps its registration; only what it shows goes.
  onDeactivated(dismiss)
  onBeforeUnmount(() => {
    dismiss()
    const i = selectionSurfaces.indexOf(surface)
    if (i !== -1) selectionSurfaces.splice(i, 1)
  })

  return { dismiss }
}
