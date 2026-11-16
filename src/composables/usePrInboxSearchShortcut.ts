import { onBeforeUnmount, onMounted, type Ref } from 'vue'
import { IS_MAC } from '@/lib/appMenu'
import { isPrSearchShortcut } from '@/lib/prInboxSearch'

/**
 * ⌘F / Ctrl+F on the PR inbox puts the cursor in its search box instead of
 * opening the in-page find bar. Capture phase + stopPropagation so the
 * window-wide find router (useFindShortcut) does not also react. Does nothing
 * while the screen is hidden (cached view) or a modal is open.
 */
export function usePrInboxSearchShortcut(root: Ref<HTMLElement | null>, focus: () => boolean): void {
  function onKeydown(e: KeyboardEvent) {
    if (e.defaultPrevented || !isPrSearchShortcut(e, IS_MAC)) return
    const el = root.value
    if (!el || !el.isConnected || el.getClientRects().length === 0) return
    if (document.querySelector('.ui-backdrop')) return
    if (!focus()) return
    e.preventDefault()
    e.stopPropagation()
  }
  onMounted(() => window.addEventListener('keydown', onKeydown, true))
  onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown, true))
}
