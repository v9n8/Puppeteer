import { onActivated, onBeforeUnmount, onDeactivated, onMounted } from 'vue'
import { calendarKeyAction, type CalendarKeyAction } from '@/lib/calendarNow'

/**
 * Calendar shortcuts on `window`. The listener is also dropped while the view is
 * cached by <KeepAlive>, so the keys never fire from another screen.
 */
export function useCalendarKeys(run: (action: CalendarKeyAction) => void) {
  function onKeydown(e: KeyboardEvent) {
    const action = calendarKeyAction(e)
    if (!action) return
    e.preventDefault()
    run(action)
  }
  const attach = () => window.addEventListener('keydown', onKeydown)
  const detach = () => window.removeEventListener('keydown', onKeydown)
  onMounted(attach)
  onActivated(attach)
  onDeactivated(detach)
  onBeforeUnmount(detach)
}
