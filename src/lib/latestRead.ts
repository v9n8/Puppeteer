// Per-key request ordering (RT-15): when several reads of the same key are in
// flight (a refresh click, the plan_usage_updated listener, the 60 s timer),
// only the newest one may land. Without it the last *response* wins, and an
// older read finishing late puts an older number back on screen.

export interface LatestRead {
  /** Start a read of `key`; the returned check is true while it is the newest. */
  begin(key: string): () => boolean
}

export function createLatestRead(): LatestRead {
  const seq = new Map<string, number>()
  return {
    begin(key) {
      const n = (seq.get(key) ?? 0) + 1
      seq.set(key, n)
      return () => seq.get(key) === n
    },
  }
}
