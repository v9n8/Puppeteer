import { clampDragHeight } from './autoGrow.ts'

/** Rows the quote list shows before it scrolls, plus a sliver of the next one. */
const DEFAULT_ROWS = 3
const PEEK_PX = 24

/** The list never takes more than half the conversation area, so the composer stays on screen. */
function maxListHeight(rowPx: number, conversationPx: number): number {
  return Math.max(rowPx, conversationPx / 2)
}

/**
 * What the list measured on screen: each row's height (`offsetHeight`, border
 * included), the space between rows and the list's own top and bottom padding.
 */
export interface QuoteListBox {
  rows: number[]
  gap: number
  padTop: number
  padBottom: number
}

/**
 * Height the first rows need on screen: up to three rows whole, then a peek of
 * the fourth when there is one; with three rows or fewer, every row and the
 * bottom padding, so nothing scrolls.
 */
export function quoteRowsHeight(box: QuoteListBox): number {
  const shown = box.rows.slice(0, DEFAULT_ROWS)
  const rows = shown.reduce((sum, h) => sum + h, 0) + box.gap * Math.max(0, shown.length - 1)
  const tail = box.rows.length > shown.length ? box.gap + PEEK_PX : box.padBottom
  return Math.ceil(box.padTop + rows + tail)
}

/**
 * Starting height of the quote list: three rows as tall as they really are (a
 * row with a note is taller) and a peek of the fourth, capped at half the
 * conversation. Before the rows are measured, three rows of `rowPx` stand in.
 */
export function defaultQuoteListHeight(box: QuoteListBox | null, rowPx: number, conversationPx: number): number {
  const want = box?.rows.length ? quoteRowsHeight(box) : DEFAULT_ROWS * rowPx + PEEK_PX
  return clampDragHeight(want, rowPx, maxListHeight(rowPx, conversationPx))
}

/** A dragged height kept between one row and half the conversation. */
export function clampQuoteListHeight(height: number, rowPx: number, conversationPx: number): number {
  return clampDragHeight(height, rowPx, maxListHeight(rowPx, conversationPx))
}

/** One arrow-key step: up adds a row, down removes one. */
export function stepQuoteListHeight(height: number, dir: 'up' | 'down', rowPx: number, conversationPx: number): number {
  return clampQuoteListHeight(height + (dir === 'up' ? rowPx : -rowPx), rowPx, conversationPx)
}
