// The app-update state shared by the corner toast (UpdateNotifier) and the
// Settings › About "Updates" card: one module-level state, so both change
// together and there is one `check_update` caller and one 12-hour timer.
import { computed, ref } from 'vue'
import { i18n } from '@/i18n'
import { invoke } from '@/lib/tauri'
import { updateStatusFrom, type UpdateCheckResult, type UpdateState } from '@/lib/updateStatus'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'

const CHECK_INTERVAL_MS = 12 * 60 * 60 * 1000

const state = ref<UpdateState>({ status: 'idle', version: null, error: null })
const lastCheckedAt = ref<number | null>(null)
const installing = ref(false)
// "Later" hides the toast for this version until the next launch; the card still
// offers it, and a newer version shows the toast again.
const dismissed = ref<string | null>(null)
let timer: number | null = null

function stopAutoCheck() {
  if (timer !== null) window.clearInterval(timer)
  timer = null
}

/** Asks the backend; never throws: a failure is the `error` state. */
async function check(): Promise<UpdateState> {
  if (state.value.status === 'checking') return state.value
  state.value = { ...state.value, status: 'checking' }
  try {
    state.value = updateStatusFrom(await invoke<UpdateCheckResult>('check_update'))
    if (state.value.status === 'unsupported') stopAutoCheck()
  } catch (e) {
    // Offline, or the release has no manifest yet: the toast stays quiet, the card shows it.
    console.warn('check_update failed', e)
    state.value = updateStatusFrom({ error: e })
  }
  lastCheckedAt.value = Date.now()
  return state.value
}

function later() {
  dismissed.value = state.value.version
}

/** Asks the user, then downloads and installs the offered release. */
async function install() {
  const version = state.value.version
  if (!version || installing.value) return
  const { t } = i18n.global
  const { confirm } = useConfirm()
  const { toast } = useToast()
  const ok = await confirm({
    title: t('update.confirmTitle', { version }),
    message: t('update.confirmMessage'),
    confirmLabel: t('update.install'),
    variant: 'primary',
  })
  if (!ok) return
  installing.value = true
  try {
    const installed = await invoke<string>('install_update')
    state.value = { status: 'up_to_date', version: null, error: null }
    dismissed.value = installed
    toast.success(t('update.installed', { version: installed }), 8000)
  } catch (e) {
    toast.error(t('update.failed', { error: String(e) }))
  } finally {
    installing.value = false
  }
}

/** Checks now and every 12 hours; returns the stop function. Only UpdateNotifier calls it. */
function startAutoCheck(): () => void {
  stopAutoCheck()
  timer = window.setInterval(() => void check(), CHECK_INTERVAL_MS)
  void check()
  return stopAutoCheck
}

/** The version the corner toast offers, or null (none, or dismissed with "Later"). */
// `version` is kept while a re-check runs, so the toast does not blink.
const toastVersion = computed(() => (state.value.version !== dismissed.value ? state.value.version : null))

export function useUpdateCheck() {
  return { state, lastCheckedAt, installing, toastVersion, check, install, later, startAutoCheck }
}
