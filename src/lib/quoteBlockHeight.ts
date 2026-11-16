import { clampDragHeight } from './autoGrow.ts'

/** Rows the quote list shows before it scrolls, plus a sliver of the next one. */
const DEFAULT_ROWS = 3
const PEEK_PX = 24

/** The list never takes more than half the conversation area, so the composer stays on screen. */
function maxListHeight(rowPx: number, conversationPx: number): number {
  return Math.max(rowPx, conversationPx / 2)
}

/** Starting height of the quote list: three rows and a peek of the fourth, capped at half the conversation. */
export function defaultQuoteListHeight(rowPx: number, conversationPx: number): number {
  return clampDragHeight(DEFAULT_ROWS * rowPx + PEEK_PX, rowPx, maxListHeight(rowPx, conversationPx))
}

/** A dragged height kept between one row and half the conversation. */
export function clampQuoteListHeight(height: number, rowPx: number, conversationPx: number): number {
  return clampDragHeight(height, rowPx, maxListHeight(rowPx, conversationPx))
}

/** One arrow-key step: up adds a row, down removes one. */
export function stepQuoteListHeight(height: number, dir: 'up' | 'down', rowPx: number, conversationPx: number): number {
  return clampQuoteListHeight(height + (dir === 'up' ? rowPx : -rowPx), rowPx, conversationPx)
}
