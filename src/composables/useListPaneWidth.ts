import { ref, type Ref } from 'vue'
import { clampPaneWidth, parsePaneWidth } from '@/lib/listPaneWidth'

/**
 * The remembered width of a list pane. `drag` changes it live; `commit`
 * stores it (once, on release or key) under `<key>.width`.
 */
export function useListPaneWidth(key: string, fallback: number): {
  width: Ref<number>
  drag: (px: number) => void
  commit: (px: number) => void
  reset: () => void
} {
  const storageKey = `${key}.width`
  let stored: string | null = null
  try {
    stored = localStorage.getItem(storageKey)
  } catch {
    /* storage unavailable */
  }
  const width = ref(parsePaneWidth(stored, fallback))

  const drag = (px: number) => {
    width.value = clampPaneWidth(px)
  }
  const commit = (px: number) => {
    drag(px)
    try {
      localStorage.setItem(storageKey, String(width.value))
    } catch {
      /* storage unavailable */
    }
  }
  const reset = () => {
    width.value = clampPaneWidth(fallback)
    try {
      localStorage.removeItem(storageKey)
    } catch {
      /* storage unavailable */
    }
  }
  return { width, drag, commit, reset }
}
