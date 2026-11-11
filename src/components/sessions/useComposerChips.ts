// The composer's permission mode / effort / response style chips (UX-26). A pick
// belongs to the session on screen and goes out with its next start or resume —
// the engines take these only when a turn launches — so changing one while a
// session runs applies from the next turn. Defaults and choices:
// lib/composerChips. Permission mode keeps its shared, remote-synced selector
// (useRunSessionCore); effort and style picks live here, per session. The
// account · model chip's menu (UX-47) opens here; its rows: useRunComposer.
import { ref, computed, watch } from 'vue'
import { ShieldQuestion, ShieldAlert, ListChecks } from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import {
  parsePercentSetting, resetCountdown, usageSummary, usageWindows, windowSeverity,
  type PlanUsageSnapshot, type UsageSeverity, type UsageThresholds, type UsageWindow,
} from '@/lib/usageRing'
import {
  EFFORT_LEVELS, PERMISSION_MODE_OPTIONS, effortLevelsFor, resolvePermissionMode, type SelectOption,
} from '@/lib/engineOptions'
import { defaultEffortChoices } from '@/lib/sessionDefaults'
import { shownStyles } from '@/lib/responseStyles'
import {
  composerStyleChoices, effectiveEffort, effortForLaunch, inheritedEffort, inheritedModel, inheritedStyle,
} from '@/lib/composerChips'
import { useResponseStylesStore } from '@/stores/responseStyles'
import type { RunSessionCore } from './useRunSessionCore'

export function useComposerChips(core: RunSessionCore) {
  const {
    t, modelCatalog, projectId, currentRunId, currentRun, currentStatus, loadedRunEngine,
    effectiveEngine, sessionDefaults: defaults, modelOverride, permissionMode, claudeStore, appSettings,
  } = core
  const stylesStore = useResponseStylesStore()

  // A session that has run continues on its own engine (as sendMessage does).
  const chipEngine = computed(() =>
    (currentStatus.value !== 'fetched' && (loadedRunEngine.value || currentRun.value?.engine)) || effectiveEngine.value,
  )
  // Levels the next turn's model takes: the model pick, else the model the run
  // last ran with, else the default model (project, then global) — what a start
  // or resume falls back to.
  function effortLevels(engine: string) {
    const model = modelOverride.value || inheritedModel(currentRun.value, engine, defaults.value)
    return effortLevelsFor(engine, model, { codex: modelCatalog.codexModels })
  }

  // Picks per session ('' = none: the default applies). A pick is sent with the
  // next launch, which stores the effort on the run; the pick itself lasts as
  // long as this view.
  const effortPicks = ref<Record<string, string>>({})
  const stylePicks = ref<Record<string, string>>({})
  function currentPick(picks: typeof effortPicks) {
    return computed({
      get: () => (currentRunId.value ? picks.value[currentRunId.value] ?? '' : ''),
      set: (value: string) => {
        if (currentRunId.value) picks.value = { ...picks.value, [currentRunId.value]: value }
      },
    })
  }
  const effortPick = currentPick(effortPicks)
  const stylePick = currentPick(stylePicks)

  // What a launch on `engine` sends. Read before a launch switches the session
  // on screen (an engine handoff), so the picks follow it.
  function chipOverrides(engine: string): { effort?: string; style?: string } {
    return { effort: effortForLaunch(effortPick.value, effortLevels(engine)), style: stylePick.value || undefined }
  }
  function carryPicks(fromRunId: string, toRunId: string) {
    if (fromRunId === toRunId) return
    effortPicks.value = { ...effortPicks.value, [toRunId]: effortPicks.value[fromRunId] ?? '' }
    stylePicks.value = { ...stylePicks.value, [toRunId]: stylePicks.value[fromRunId] ?? '' }
  }

  const started = computed(() => !!currentRunId.value && currentStatus.value !== 'fetched')
  function chipTitle(name: string, value: string, fromDefault: boolean) {
    const base = t(fromDefault ? 'run.chipTitleDefault' : 'run.chipTitle', { name, value })
    return started.value ? `${base} ${t('run.chipNextTurn')}` : base
  }

  // ── Permission mode ──
  const modeValue = computed(() => resolvePermissionMode(permissionMode.value, defaults.value.permissionMode))
  const modeChipLabel = computed(() => t(`run.chipModeShort.${modeValue.value}`))
  const modeChipIcon = computed(() =>
    modeValue.value === 'bypassPermissions' ? ShieldAlert : modeValue.value === 'plan' ? ListChecks : ShieldQuestion,
  )
  const modeChipTitle = computed(() => {
    const full = PERMISSION_MODE_OPTIONS.find((o) => o.value === modeValue.value)?.label ?? modeChipLabel.value
    return chipTitle(t('run.chipMode'), full, !permissionMode.value)
  })

  // ── Effort ──
  const effortLevelsNow = computed(() => effortLevels(chipEngine.value))
  // No pick: the level the run last ran with (a resume keeps it), else the default.
  const effortDefault = computed(() => inheritedEffort(currentRun.value?.effort, defaults.value.effort))
  const effortValue = computed(() => effectiveEffort(effortPick.value, effortLevelsNow.value, effortDefault.value))
  const effortLabel = (level: string) =>
    (EFFORT_LEVELS as readonly string[]).includes(level) ? t(`settings.ai.effort.${level}`) : level
  const effortOptions = computed<SelectOption[]>(() =>
    defaultEffortChoices(effortLevelsNow.value, effortPick.value).map((c) => {
      if (!c.value) {
        const d = effectiveEffort('', effortLevelsNow.value, effortDefault.value)
        return { value: '', label: d ? t('run.chipEffortDefaultWith', { level: effortLabel(d) }) : t('settings.ai.effortEngineDefault') }
      }
      const label = effortLabel(c.value)
      return c.supported ? { value: c.value, label } : { value: c.value, label, description: t('settings.ai.effortUnsupported') }
    }),
  )
  const effortChipLabel = computed(() => (effortValue.value ? effortLabel(effortValue.value) : t('run.chipEffortEngine')))
  const effortChipTitle = computed(() =>
    chipTitle(t('run.chipEffort'), effortValue.value ? effortLabel(effortValue.value) : t('settings.ai.effortEngineDefault'), !effortPick.value),
  )

  // ── Response style ──
  watch(projectId, (id) => {
    stylesStore.load(id ?? null).catch(() => {})
  }, { immediate: true })
  const knownStyle = computed(() =>
    stylesStore.loaded
      ? (id: string) => stylesStore.styles.some((s) => s.id === id && (s.project_id === null || s.project_id === projectId.value))
      : null,
  )
  // No pick: the run's stored style (a resume keeps it), else the defaults.
  const styleValue = computed(() =>
    stylePick.value || inheritedStyle(currentRun.value?.style, defaults.value.styles, knownStyle.value),
  )
  // The global styles and this project's under their own headings.
  const styleOptions = computed(() => {
    const out: (SelectOption & { header?: boolean })[] = []
    let group = ''
    const heading = (id: string, label: string) => {
      if (group === id) return
      group = id
      out.push({ value: `__group:${id}`, label, header: true })
    }
    for (const c of composerStyleChoices(shownStyles(stylesStore.styles, t), projectId.value, styleValue.value)) {
      if (c.kind === 'none') {
        out.push({ value: c.value, label: t('settings.ai.styleNone'), description: t('settings.ai.styleNoneHint') })
      } else if (c.kind === 'mine' || c.kind === 'project') {
        heading(c.kind, c.kind === 'mine' ? t('responseStyles.mineTitle') : t('run.chipStyleProject'))
        out.push(c.description ? { value: c.value, label: c.name, description: c.description } : { value: c.value, label: c.name })
      } else {
        // Before the list loads a style id cannot be named yet; after, it is gone.
        out.push({ value: c.value, label: stylesStore.loaded ? t('settings.ai.styleMissing') : '…' })
      }
    }
    return out
  })
  const styleChipLabel = computed(() => styleOptions.value.find((o) => !o.header && o.value === styleValue.value)?.label ?? '')
  const styleChipTitle = computed(() => chipTitle(t('run.chipStyle'), styleChipLabel.value, !stylePick.value))

  // ── Account · model (UX-47) ──
  // One menu, two tabs. The status bar's account · model chip opens it too
  // (data-account-menu-trigger keeps the outside-click below from closing it
  // before that chip's click toggles it).
  const accountMenuOpen = ref(false)
  const accountMenuRoot = ref<HTMLElement | null>(null)
  const accountTab = ref<'model' | 'account'>('account')
  function toggleAccountMenu() {
    accountMenuOpen.value = !accountMenuOpen.value
  }
  function closeAccountMenu() {
    accountMenuOpen.value = false
  }
  function onAccountMenuPointerDown(e: MouseEvent) {
    if (!accountMenuOpen.value) return
    const target = e.target
    if (!(target instanceof Element)) return
    if (accountMenuRoot.value?.contains(target)) return
    if (target.closest('[data-account-menu-trigger]')) return
    closeAccountMenu()
  }

  // Plan usage per row key (`claude:<id>`, `claude`, `codex`, as the status
  // bar's usage rings key them), read from the stored snapshots each time the
  // menu opens: local reads, no engine call.
  const usageSnapshots = ref<Record<string, PlanUsageSnapshot | null>>({})
  const usageNow = ref(Date.now())
  async function loadUsage(key: string) {
    try {
      usageSnapshots.value[key] = key === 'codex'
        ? await invoke<PlanUsageSnapshot | null>('get_plan_usage_codex')
        : key === 'claude'
          ? await invoke<PlanUsageSnapshot | null>('get_plan_usage')
          : await invoke<PlanUsageSnapshot | null>('get_plan_usage', { accountId: key.slice('claude:'.length) })
    } catch {
      // keep the previous figures on a transient error
    }
  }
  watch(accountMenuOpen, (open) => {
    if (!open) return
    usageNow.value = Date.now()
    const keys = ['codex', 'claude', ...claudeStore.accounts.map((a) => `claude:${a.id}`)]
    void Promise.all(keys.map(loadUsage))
  })
  const usageThresholds = computed<UsageThresholds>(() => ({
    warnPercent: parsePercentSetting(appSettings.settings?.quota_warn_percent),
    block5hPercent: parsePercentSetting(appSettings.settings?.budget_5h_percent),
    blockWeekPercent: parsePercentSetting(appSettings.settings?.budget_week_percent),
  }))
  const SEVERITY_TEXT: Record<UsageSeverity, string> = { ok: 'text-info', warn: 'text-warning', over: 'text-destructive' }
  function usageResetText(w: UsageWindow): string {
    if (w.rolledOver) return t('misc.budget.windowReset')
    const c = resetCountdown(w.reset, usageNow.value)
    if (!c) return ''
    if (c.due) return t('misc.budget.resetsSoon')
    if (c.days > 0) return c.hours > 0 ? t('misc.budget.resetsInDaysHours', c) : t('misc.budget.resetsInDays', c)
    if (c.hours > 0) return c.minutes > 0 ? t('misc.budget.resetsInHoursMins', c) : t('misc.budget.resetsInHours', c)
    return t('misc.budget.resetsInMins', c)
  }
  /** The 5h and weekly figures of one usage row, coloured like its ring. */
  function accountUsage(key: string | null) {
    if (!key) return []
    return usageSummary(usageWindows(usageSnapshots.value[key], usageNow.value)).map((w) => ({
      key: w.key,
      label: t(w.period === '5h' ? 'run.accountMenu.fiveHour' : 'run.accountMenu.week'),
      percent: w.percent,
      color: SEVERITY_TEXT[windowSeverity(w, usageThresholds.value)],
      reset: usageResetText(w),
      title: `${t(`misc.budget.windows.${w.key}`)}: ${w.percent}%`,
    }))
  }

  return {
    chipOverrides, carryPicks,
    accountMenuOpen, accountMenuRoot, accountTab, toggleAccountMenu, closeAccountMenu, onAccountMenuPointerDown,
    accountUsage,
    modeValue, modeChipLabel, modeChipIcon, modeChipTitle,
    effortPick, effortLevelsNow, effortOptions, effortChipLabel, effortChipTitle,
    stylePick, styleValue, styleOptions, styleChipLabel, styleChipTitle,
  }
}

export type ComposerChips = ReturnType<typeof useComposerChips>
