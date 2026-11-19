// The one-time notice of an automatic routing policy version (QO-FR48): a toast
// with what moved and a button to Settings › Routing. The backend keeps the
// notice until it is taken (`take_routing_notice`), so one made before this
// window was ready is still shown, and only once. Main window only (App.vue,
// past the pop-out and session-window early returns).
import { onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import { useToast } from '@/composables/useToast'
import { ROUTING_UPDATED_EVENT, isNewNotice, noticeSummary, type RoutingNotice } from '@/lib/routing'
import { settingsLocation } from '@/lib/settingsSections'

/** Long enough to read a few moves. */
const NOTICE_MS = 12000

/** Returns the start: call it once the main window is mounted. */
export function useRoutingNotice(): () => Promise<void> {
  const router = useRouter()
  const { t } = useI18n()
  const { toast } = useToast()
  let lastShown = 0
  let unlisten: UnlistenFn | null = null

  async function take() {
    let notice: RoutingNotice | null
    try {
      notice = await invoke<RoutingNotice | null>('take_routing_notice')
    } catch (e) {
      console.warn('[routing] take_routing_notice failed', e)
      return
    }
    if (!isNewNotice(notice, lastShown)) return
    lastShown = notice.version
    toast.info(t('routing.notice.updated', { n: notice.version, summary: noticeSummary(notice.changes) }), NOTICE_MS, {
      label: t('routing.notice.open'),
      run: () => void router.push(settingsLocation({ section: 'routing' })).catch(() => {}),
    })
  }

  onBeforeUnmount(() => unlisten?.())

  return async () => {
    // Listen first, so a version made between the two is not missed.
    try {
      unlisten = await listen(ROUTING_UPDATED_EVENT, () => void take())
    } catch {
      // Outside the Tauri shell (browser dev): only the start-up read.
    }
    await take()
  }
}
