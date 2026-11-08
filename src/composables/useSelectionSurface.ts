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

/** Where a quote goes; `label` names it on the button. */
export interface QuoteTarget {
  label: string
  append: (text: string) => void
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
}

/** A selection the bar is shown for. */
export interface SelectionBar {
  surface: SelectionSurface
  text: string
  rect: { left: number; top: number; bottom: number }
  quoteTarget: QuoteTarget | null
}

/** A translation popover opened from the bar. */
export interface SelectionTranslation {
  surface: SelectionSurface
  text: string
  x: number
  y: number
}

/** Surfaces declared in this window, in mount order. */
export const selectionSurfaces: SelectionSurface[] = []
export const selectionBar = shallowRef<SelectionBar | null>(null)
export const selectionTranslation = shallowRef<SelectionTranslation | null>(null)

export function surfaceEls(surface: SelectionSurface): (HTMLElement | null)[] {
  return (Array.isArray(surface.el) ? surface.el : [surface.el]).map((r) => r.value)
}

/**
 * Register `surface` for the calling component's lifetime. `dismiss` hides the
 * bar and closes the translation if they belong to it — for when its content
 * is replaced (another run, another file).
 */
export function useSelectionSurface(surface: SelectionSurface): { dismiss: () => void } {
  selectionSurfaces.push(surface)

  function dismiss() {
    if (selectionBar.value?.surface === surface) selectionBar.value = null
    if (selectionTranslation.value?.surface === surface) selectionTranslation.value = null
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
