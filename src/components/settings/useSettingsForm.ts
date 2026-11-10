// The `settings` table form shared by the Settings sections: the object every
// section binds to, its load, and the debounced auto-save.
//
// SettingsView creates it once (createSettingsForm) and each section injects it
// (useSettingsForm). Per-view rather than module state, so the watchers below
// stop when the view unmounts and a reopened Settings starts from a fresh load.
import { inject, provide, ref, watch, type InjectionKey, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@/lib/tauri'
import { useToast } from '@/composables/useToast'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useFeaturesStore } from '@/stores/features'
import { errorText } from '@/lib/featureGate'
import { TERMINAL_APPS, desktopOs } from '@/lib/terminalApps'

export interface AppSettings {
  default_engine: string
  claude_path: string
  codex_path: string
  claude_model: string
  codex_model: string
  extra_args: string
  theme: string
  language: string
  color_theme: string
  animated_background: string
  default_permission_mode: string
  default_effort: string
  default_style: string
  terminal_app: string
  context_warn_percent: string
  context_limit_override: string
  budget_5h_percent: string
  budget_week_percent: string
  quota_warn_percent: string
  quota_stop_running: string
  claude_auto_balance: string
  claude_auto_balance_min_swap_minutes: string
  claude_auto_balance_swap_threshold_pct: string
  translate_engine: string
  translate_model: string
  translate_target_lang: string
  translate_style: string
  quit_background_policy: string
}

export interface SettingsForm {
  settings: Ref<AppSettings>
  /** True until the first load settles; watchers skip changes made meanwhile. */
  loading: Ref<boolean>
  /** Read the settings table and take the snapshot auto-save diffs against. */
  load: () => Promise<void>
  /** Write the keys that differ from the last snapshot (no-op when none do). */
  persistChanges: () => Promise<void>
}

const SETTINGS_FORM: InjectionKey<SettingsForm> = Symbol('settingsForm')

export function createSettingsForm(): SettingsForm {
  const { t } = useI18n()
  const { toast } = useToast()
  const appSettings = useAppSettingsStore()

  const settings = ref<AppSettings>({
    default_engine: 'claude',
    claude_path: 'claude',
    codex_path: 'codex',
    claude_model: '',
    codex_model: '',
    extra_args: '',
    theme: 'system',
    language: 'en',
    color_theme: 'default',
    animated_background: 'true',
    default_permission_mode: 'default',
    default_effort: '',
    default_style: '',
    terminal_app: TERMINAL_APPS[desktopOs(navigator.userAgent)][0].value,
    context_warn_percent: '80',
    context_limit_override: '',
    budget_5h_percent: '',
    budget_week_percent: '',
    quota_warn_percent: '',
    quota_stop_running: 'false',
    claude_auto_balance: 'true',
    claude_auto_balance_min_swap_minutes: '10',
    claude_auto_balance_swap_threshold_pct: '10',
    translate_engine: 'claude',
    translate_model: '',
    translate_target_lang: 'vi',
    translate_style: 'natural',
    quit_background_policy: 'ask',
  })
  const loading = ref(true)
  // Snapshot of the last persisted settings so the auto-save watcher only
  // pushes keys that actually changed.
  let lastSaved: AppSettings | null = null
  let saveTimer: ReturnType<typeof setTimeout> | null = null

  async function load() {
    settings.value = await invoke<AppSettings>('get_settings')
    // Older backends don't know this key yet; show the default instead of a blank select.
    settings.value.quit_background_policy ||= 'ask'
    lastSaved = { ...settings.value }
  }

  async function persistChanges() {
    if (!lastSaved) return
    const changed = Object.entries(settings.value).filter(
      ([k, v]) => String(v) !== String(lastSaved![k as keyof AppSettings]),
    )
    if (!changed.length) return
    try {
      for (const [key, value] of changed) {
        await invoke('update_setting', { key, value: String(value) })
      }
      lastSaved = { ...settings.value }
      // Keep the shared settings store (context meter + budget badge) in sync.
      appSettings.refresh().catch(() => {})
      toast.success(t('settings.toast.saved'))
    } catch (e) {
      toast.error(errorText(e, t))
    }
  }

  // Auto-save: persist (debounced) whenever a setting changes.
  watch(settings, () => {
    if (loading.value || !lastSaved) return
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(persistChanges, 400)
  }, { deep: true })

  // Settings → Features also writes `animated_background` (prop
  // `app.animated_background`): follow it here, in the copy auto-save diffs
  // against too, so the General select neither goes stale nor writes it back.
  const features = useFeaturesStore()
  watch(() => features.tree, (tree) => {
    const prop = tree.find((m) => m.module === 'app')?.props.find((p) => p.id === 'app.animated_background')
    if (!prop || !lastSaved) return
    const value = String(prop.stored ?? prop.default)
    if (lastSaved.animated_background === value) return
    settings.value.animated_background = value
    lastSaved.animated_background = value
    appSettings.refresh().catch(() => {})
  })

  const form: SettingsForm = { settings, loading, load, persistChanges }
  provide(SETTINGS_FORM, form)
  return form
}

export function useSettingsForm(): SettingsForm {
  const form = inject(SETTINGS_FORM)
  if (!form) throw new Error('useSettingsForm() needs createSettingsForm() in an ancestor (SettingsView)')
  return form
}
