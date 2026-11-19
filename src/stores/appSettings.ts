import { defineStore } from 'pinia'
import { ref } from 'vue'
import { emit, listen } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import { notifySettingsChanged, startSettingsSync } from '@/lib/settingsSync'

/**
 * Reactive mirror of the backend `AppSettings`, loaded once and shared across
 * components that need settings outside of the Settings screen (the context
 * meter and the token-budget badge). SettingsView still owns editing; it calls
 * `refresh()` after persisting so this store stays in sync.
 */
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
  default_permission_mode: string
  /** Default reasoning effort for new runs ('' = engine decides); see lib/engineOptions.ts EFFORT_LEVELS. */
  default_effort: string
  /** Default response style of new sessions ('' = none): a built-in key or a style id (lib/responseStyles). UI in UX-18. */
  default_style: string
  /** Worker cap of a new session, '1'–'100' (default '3'); read with `defaultWorkerCap` (lib/workerCap.ts). */
  default_max_workers: string
  terminal_app: string
  context_warn_percent: string
  context_limit_override: string
  budget_5h_percent: string
  budget_week_percent: string
  /** Quota warning tier in % ('' = off): toast + badge colour, never blocks. */
  quota_warn_percent: string
  /** 'true' = stop running sessions when a block threshold is reached (default 'false'). */
  quota_stop_running: string
  /** Model calls one unattended turn (a schedule's, a Remote Control start) may make ('0' = no limit, default '200'). */
  unattended_max_turns: string
  /** Minutes one unattended turn may run ('0' = no limit, default '60'). */
  unattended_max_minutes: string
  claude_auto_balance: string
  claude_auto_balance_min_swap_minutes: string
  claude_auto_balance_swap_threshold_pct: string
  translate_engine: string
  translate_model: string
  translate_target_lang: string
  translate_style: string
  mcp_builtin_puppeteer_enabled: string
  mcp_builtin_google_enabled: string
  /** What quitting does with running sessions / leftover processes: 'ask' | 'stop_all' | 'keep_leftovers'. */
  quit_background_policy: string
  /** Newest version whose "What's new" notes were offered ('' = none yet); see lib/whatsNew.ts. */
  whats_new_seen_version: string
  /** Command palette layout as JSON (the `palette.config` row); parse with `parsePaletteConfig`. */
  palette_config: string
  /** Menu shortcuts the user changed, as JSON (the `keymap.config` row); parse with `parseKeymap`. */
  keymap_config: string
  /** Desktop notification switches as JSON ('' = all on); parse with `parseDesktopNotify` (lib/desktopNotify.ts). */
  desktop_notify: string
  /** Models that are on per engine, as JSON (V3-62); parse with `parseEnabledModels` (lib/enabledModels.ts). */
  enabled_models: string
}

export const useAppSettingsStore = defineStore('appSettings', () => {
  const settings = ref<AppSettings | null>(null)
  const loaded = ref(false)

  // One listener per window for the store's lifetime: a save made in another
  // window (the Settings window) re-reads here (lib/settingsSync.ts).
  let syncing = false
  async function refresh() {
    if (!syncing) {
      syncing = true
      void startSettingsSync(listen, refresh)
    }
    settings.value = await invoke<AppSettings>('get_settings')
    loaded.value = true
  }

  /** Call after a save succeeded so the other windows re-read `keys`. */
  function notifyChanged(keys: string[]) {
    return notifySettingsChanged(emit, keys)
  }

  /** Load once; subsequent calls are no-ops (use refresh() to force-reload). */
  async function ensureLoaded() {
    if (!loaded.value) await refresh()
  }

  return { settings, loaded, refresh, ensureLoaded, notifyChanged }
})
