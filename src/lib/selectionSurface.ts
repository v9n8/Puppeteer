// The selection toolbar's pure rules (components/SelectionToolbar.vue,
// composables/useSelectionSurface.ts): which declared surface owns a selection,
// which buttons it gets, what Copy MD copies, where the bar sits. Kept free of
// Vue / Tauri / `@/` imports so it can be unit-tested with `node --test`.
import { isFeatureOn, type FeatureTree } from './featureGate.ts'
import { rawMarkdownForSelection } from './selectionMarkdown.ts'

export type SelectionAction = 'quote' | 'translate' | 'copyMd' | 'saveNote' | 'saveTodo'

/** Button order on the bar. */
export const SELECTION_ACTIONS: readonly SelectionAction[] = [
  'quote',
  'translate',
  'copyMd',
  'saveNote',
  'saveTodo',
]

/**
 * Features each button needs, all of them on. Saving a note / todo is useless
 * without the notes / todos feature, even though the registry only marks it as
 * a dependency (the prop itself stays on, degraded).
 */
export const ACTION_FEATURES: Readonly<Record<SelectionAction, readonly string[]>> = {
  quote: ['text_tools.selection.quote'],
  translate: ['text_tools.selection.translate'],
  copyMd: ['text_tools.selection.copy_md'],
  saveNote: ['text_tools.selection.save_note', 'productivity.notes'],
  saveTodo: ['text_tools.selection.save_todo', 'productivity.todos'],
}

/** What a surface offers; a missing entry is off. */
export type SelectionAllow = Partial<Record<SelectionAction, boolean>>

// Labels of the Secret vault window (lib/secretNotesWindow.ts), local and as
// opened from a remote-desktop window (lib/popout.ts).
const EXCLUDED_LABELS: readonly string[] = ['secret-notes', 'remote-secret-notes']

/**
 * Windows that never get the bar: the Secret vault, whose text must not be sent
 * to a model (Translate) or copied out by a shortcut.
 */
export function isSelectionExcludedWindow(search: string, label: string | null): boolean {
  if (new URLSearchParams(search).get('secretNotesWindow') === '1') return true
  return label !== null && EXCLUDED_LABELS.includes(label)
}

export interface ActionContext {
  /** This window is excluded (`isSelectionExcludedWindow`). */
  excludedWindow: boolean
  /** The surface has somewhere to send a quote right now. */
  hasQuoteTarget: boolean
}

/** Buttons to show, in bar order. Empty = no bar. */
export function selectionActions(
  tree: FeatureTree,
  allow: SelectionAllow,
  ctx: ActionContext,
): SelectionAction[] {
  if (ctx.excludedWindow) return []
  return SELECTION_ACTIONS.filter(
    (action) =>
      allow[action] === true &&
      (action !== 'quote' || ctx.hasQuoteTarget) &&
      ACTION_FEATURES[action].every((id) => isFeatureOn(tree, id)),
  )
}

/**
 * Whether the bar may reach the translation model for a surface — warm it
 * (`prewarm_translate`) or send it text (`translate_text`): only while that
 * surface's Translate button would show. Translate sends the text to a model,
 * so the vault and a switched-off `selection.translate` never get there.
 */
export function canTranslate(tree: FeatureTree, allow: SelectionAllow, excludedWindow: boolean): boolean {
  return selectionActions(tree, allow, { excludedWindow, hasQuoteTarget: false }).includes('translate')
}

/**
 * The surface owning a selection: one of whose elements holds its anchor or
 * focus. When surfaces nest, the innermost wins.
 */
export function pickSurface<S>(
  surfaces: readonly S[],
  elsOf: (surface: S) => readonly (Node | null | undefined)[],
  anchor: Node | null,
  focus: Node | null,
): S | null {
  const hits: { surface: S; el: Node }[] = []
  for (const surface of surfaces) {
    const el = elsOf(surface).find(
      (e): e is Node => !!e && ((!!anchor && e.contains(anchor)) || (!!focus && e.contains(focus))),
    )
    if (el) hits.push({ surface, el })
  }
  const inner = hits.find((h) => !hits.some((o) => o.el !== h.el && h.el.contains(o.el)))
  return inner?.surface ?? null
}

/**
 * What Copy MD puts on the clipboard: the raw markdown behind `selected` in the
 * surface's sources, or the selected text itself when no source holds it.
 */
export function copyMdText(sources: readonly string[], selected: string): string {
  return rawMarkdownForSelection(sources, selected) ?? selected
}

/**
 * A run transcript's Copy MD sources: the markdown its messages render from
 * (assistant text, a result's own text), in transcript order.
 */
export function transcriptMarkdownSources(
  entries: readonly { kind: string; text?: string }[],
): string[] {
  return entries.flatMap((e) => ((e.kind === 'text' || e.kind === 'result') && e.text ? [e.text] : []))
}

/**
 * Whether scrolling `target` moves a surface's text (so the bar, anchored to a
 * viewport rect, is stale): the document, an ancestor of the surface, or a
 * scroller inside it.
 */
export function scrollMovesSurface(
  target: Node | null,
  els: readonly (Node | null | undefined)[],
  doc: Node,
): boolean {
  if (!target || target === doc) return true
  return els.some((el) => !!el && (target.contains(el) || el.contains(target)))
}

export interface Box {
  width: number
  height: number
}

const GAP = 6
const MARGIN = 8

/**
 * Top-left of the bar for a selection rect: under it, left-aligned; above it
 * when there is no room below; always kept inside the viewport.
 */
export function toolbarPosition(
  rect: { left: number; top: number; bottom: number },
  bar: Box,
  viewport: Box,
): { x: number; y: number } {
  let y = rect.bottom + GAP
  if (y + bar.height > viewport.height - MARGIN) y = rect.top - GAP - bar.height
  const x = Math.max(MARGIN, Math.min(rect.left, viewport.width - bar.width - MARGIN))
  y = Math.max(MARGIN, Math.min(y, viewport.height - bar.height - MARGIN))
  return { x, y }
}
