// Two-column view of a line diff (V3-47): pairs the removed and added lines of
// a change so they sit side by side. Pure; the rows come from `computeDiff`.
import type { DiffRow } from '@/lib/diff'

export interface SplitCell {
  type: 'ctx' | 'add' | 'del'
  text: string
  no: number
}

export type SplitRow =
  | { type: 'fold'; count: number }
  | { type: 'pair'; left: SplitCell | null; right: SplitCell | null }

export function splitDiffRows(rows: DiffRow[]): SplitRow[] {
  const out: SplitRow[] = []
  let dels: SplitCell[] = []
  let adds: SplitCell[] = []
  // A change is a run of `del` / `add` rows; its i-th removed line sits beside its i-th added one.
  const flush = () => {
    const n = Math.max(dels.length, adds.length)
    for (let i = 0; i < n; i++) out.push({ type: 'pair', left: dels[i] ?? null, right: adds[i] ?? null })
    dels = []
    adds = []
  }
  for (const row of rows) {
    if (row.type === 'del') dels.push({ type: 'del', text: row.text, no: row.oldNo ?? 0 })
    else if (row.type === 'add') adds.push({ type: 'add', text: row.text, no: row.newNo ?? 0 })
    else {
      flush()
      if (row.type === 'fold') out.push({ type: 'fold', count: row.count })
      else {
        out.push({
          type: 'pair',
          left: { type: 'ctx', text: row.text, no: row.oldNo ?? 0 },
          right: { type: 'ctx', text: row.text, no: row.newNo ?? 0 },
        })
      }
    }
  }
  flush()
  return out
}
