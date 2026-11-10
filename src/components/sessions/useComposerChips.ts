// The composer's permission mode / effort / response style chips (UX-26). A pick
// belongs to the session on screen and goes out with its next start or resume —
// the engines take these only when a turn launches — so changing one while a
// session runs applies from the next turn. Defaults and choices:
// lib/composerChips. Permission mode keeps its shared, remote-synced selector
// (useRunSessionCore); effort and style picks live here, per session.
import { ref, computed, watch } from 'vue'
import { ShieldQuestion, ShieldAlert, ListChecks } from 'lucide-vue-next'
import {
  EFFORT_LEVELS, PERMISSION_MODE_OPTIONS, effortLevelsFor, resolvePermissionMode, type SelectOption,
} from '@/lib/engineOptions'
import { defaultEffortChoices } from '@/lib/sessionDefaults'
import { builtinStyleLabelKey } from '@/lib/responseStyles'
import {
  composerStyleChoices, effectiveEffort, effortForLaunch, inheritedEffort, inheritedModel, inheritedStyle,
} from '@/lib/composerChips'
import { useResponseStylesStore } from '@/stores/responseStyles'
import type { RunSessionCore } from './useRunSessionCore'

export function useComposerChips(core: RunSessionCore) {
  const {
    t, modelCatalog, projectId, currentRunId, currentRun, currentStatus, loadedRunEngine,
    effectiveEngine, sessionDefaults: defaults, modelOverride, permissionMode,
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
  // Built-ins, My styles and this project's styles under their own headings.
  const styleOptions = computed(() => {
    const out: (SelectOption & { header?: boolean })[] = []
    let group = ''
    const heading = (id: string, label: string) => {
      if (group === id) return
      group = id
      out.push({ value: `__group:${id}`, label, header: true })
    }
    for (const c of composerStyleChoices(stylesStore.styles, projectId.value, styleValue.value)) {
      if (c.kind === 'none') {
        out.push({ value: c.value, label: t('settings.ai.styleNone'), description: t('settings.ai.styleNoneHint') })
      } else if (c.kind === 'builtin') {
        heading('builtin', t('responseStyles.builtinTitle'))
        out.push({ value: c.value, label: t(builtinStyleLabelKey(c.value)), description: t(`responseStyles.builtin.${c.value}.description`) })
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

  return {
    chipOverrides, carryPicks,
    modeValue, modeChipLabel, modeChipIcon, modeChipTitle,
    effortPick, effortLevelsNow, effortOptions, effortChipLabel, effortChipTitle,
    stylePick, styleValue, styleOptions, styleChipLabel, styleChipTitle,
  }
}

export type ComposerChips = ReturnType<typeof useComposerChips>
