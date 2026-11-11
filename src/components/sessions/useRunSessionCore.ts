// The run screen's shared core: stores, the route's project / run, the run on
// screen with its live session, and the per-run engine / model / permission
// selection (synced with a remote controller).
//
// Part of the RunView split (UX-03): createRunSession (useRunSession.ts) calls
// this first and hands the result to the other parts.
import { ref, computed, onUnmounted, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useProjectsStore } from '@/stores/projects'
import { useRunsStore, type RunRecord } from '@/stores/runs'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { useChatDraftsStore } from '@/stores/chatDrafts'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useServersStore } from '@/stores/servers'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useAzureAccountsStore } from '@/stores/azureAccounts'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useFeaturesStore } from '@/stores/features'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useUILayoutStore } from '@/stores/uiLayout'
import { invoke } from '@/lib/tauri'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useConductorStore } from '@/stores/conductor'
import { useConfirm } from '@/composables/useConfirm'
import { usePrompt } from '@/composables/usePrompt'
import { useToast } from '@/composables/useToast'
import { IS_SESSION_WINDOW } from '@/lib/windowMode'
import { composerDefaults } from '@/lib/composerChips'

export function useRunSessionCore() {
  const { t } = useI18n()
  const route = useRoute()
  const router = useRouter()
  const projectStore = useProjectsStore()
  const runsStore = useRunsStore()
  const live = useLiveRunsStore()
  const conductorStore = useConductorStore()
  const tabsStore = useWorkspaceTabsStore()
  const draftsStore = useChatDraftsStore()
  const ghStore = useGithubAccountsStore()
  const glStore = useGitlabAccountsStore()
  const serversStore = useServersStore()
  const awsStore = useAwsAccountsStore()
  const azureStore = useAzureAccountsStore()
  const claudeStore = useClaudeAccountsStore()
  const codexStore = useCodexAccountsStore()
  const appSettings = useAppSettingsStore()
  const features = useFeaturesStore()
  const modelCatalog = useModelCatalogStore()
  const uiLayout = useUILayoutStore()
  const { confirm } = useConfirm()
  const { prompt } = usePrompt()
  const { toast } = useToast()

  const projectId = computed(() => route.params.projectId as string)
  const project = computed(() => projectStore.projects.find(p => p.id === projectId.value))

  // True when THIS webview is a standalone session pop-out (see lib/sessionWindow).
  // Such a window is locked to its one run: the session-switching chrome (left rail
  // + the navigation buttons in the toolbar) is hidden so it stays a single
  // conversation, and its own "pop out" button is suppressed. Read from windowMode
  // (captured at module load) rather than the live URL, which the router rewrites
  // once it navigates this window to the run.
  const isSessionWindow = IS_SESSION_WINDOW

  const activeRunId = computed(() => route.params.runId as string | undefined)

  const engineOverride = ref('')
  const modelOverride = ref('')
  // Engine that produced the currently loaded conversation. The selector can
  // temporarily hold a not-yet-started override, so keep the persisted run engine
  // explicit for labels, resume, and handoff decisions.
  const loadedRunEngine = ref<string>('')
  // The project's defaults for new sessions over the global ones (UX-19).
  const sessionDefaults = computed(() =>
    composerDefaults({ settings: appSettings.settings, project: project.value?.session_defaults }),
  )
  function resolveEngineChoice(value: string | null | undefined): string {
    return value || sessionDefaults.value.engine
  }
  function syncLoadedRunEngine(engine: string) {
    engineOverride.value = engine
    loadedRunEngine.value = engine
  }
  // Effective engine for the next start/handoff (selector value, else default).
  const effectiveEngine = computed(() => resolveEngineChoice(engineOverride.value))
  const permissionMode = ref('')

  // ── Run selectors: the defaults, or a per-session pick ──────────────────────
  // An empty selector value means "follow the defaults" (the project's, else the
  // global setting; reactive), and any explicit pick applies to THIS run only.
  // Per-run choices are persisted in the run's own meta (`set_run_meta` / the
  // `run:meta:<id>` listener below), never project-wide, so one run's override
  // never leaks into another session.

  const currentRunId = ref<string | null>(null)

  // ── Remote meta sync ────────────────────────────────────────────────────────
  // The composer's engine / model / permission-mode selection is the single source
  // of truth shared with a remote controller. Local edits are pushed to the backend
  // store (which fans them out to the controller); controller-originated edits are
  // applied back onto these selectors. `applyingRemoteMeta` guards the echo so an
  // applied value doesn't bounce straight back out.
  let applyingRemoteMeta = false
  let metaUnlisten: UnlistenFn | null = null
  function pushRunMeta(): void {
    if (applyingRemoteMeta) return
    const id = currentRunId.value
    if (!id) return
    void invoke('set_run_meta', {
      runId: id,
      engine: effectiveEngine.value || null,
      model: modelOverride.value || null,
      permissionMode: permissionMode.value || null,
    })
  }
  watch([engineOverride, modelOverride, permissionMode], () => pushRunMeta())
  watch(
    currentRunId,
    async (id) => {
      if (metaUnlisten) {
        metaUnlisten()
        metaUnlisten = null
      }
      if (!id) return
      metaUnlisten = await listen<{
        engine?: string | null
        model?: string | null
        permission_mode?: string | null
      }>(`run:meta:${id}`, (e) => {
        const m = e.payload
        applyingRemoteMeta = true
        if (m.engine) engineOverride.value = m.engine
        if (typeof m.model === 'string') modelOverride.value = m.model
        if (typeof m.permission_mode === 'string') permissionMode.value = m.permission_mode
        void nextTick(() => {
          applyingRemoteMeta = false
        })
      })
      // Seed the store with the current selection so a controller pairing right now
      // lands on the correct engine instead of a blank/default.
      pushRunMeta()
    },
    { immediate: true },
  )

  const currentRun = computed(() => runsStore.runs.find(r => r.id === currentRunId.value))

  const currentIsSession = computed(() => currentRun.value?.run_type === 'session')

  watch(
    () => [currentRun.value?.id, currentRun.value?.engine] as const,
    ([id, engine]) => {
      if (id && id === currentRunId.value && engine) syncLoadedRunEngine(engine)
    },
  )

  // Live streaming state for the focused run is owned by the liveRuns store so it
  // survives navigation and lets multiple runs stream at once.
  const session = computed(() => currentRunId.value ? live.get(currentRunId.value) : undefined)
  // Viewing a run acknowledges its "run finished" notification in the Active runs
  // dock. Fires when the focused run changes and when it finishes while on screen.
  watch(
    [currentRunId, () => session.value?.notifyDone],
    () => {
      if (currentRunId.value && session.value?.notifyDone) live.markSeen(currentRunId.value)
    },
    { immediate: true },
  )
  const liveEntries = computed(() => session.value?.entries ?? [])
  const liveOutputLines = computed(() => session.value?.outputLines ?? [])
  const liveHasStream = computed(() => session.value?.hasStreamEvents ?? false)
  // Prefer the live session's status, fall back to the persisted run row.
  const currentStatus = computed(() => session.value?.status ?? currentRun.value?.status ?? 'idle')
  const currentSessionId = computed(() => session.value?.sessionId ?? currentRun.value?.session_id ?? null)

  // Sidebar/label text for a run: its title when set (sessions, or a PR inbox
  // review titled with its author); otherwise issue/PR show "#N".
  function runLabel(run: RunRecord): string {
    if (run.title?.trim()) return run.title.trim()
    if (run.run_type === 'session') return t('run.labelSession')
    return `${run.run_type === 'analyze_issue' ? t('run.labelIssue') : t('run.labelPr')} #${run.ref_number}`
  }

  onUnmounted(() => {
    metaUnlisten?.()
    metaUnlisten = null
  })

  return {
    t, route, router, projectStore, runsStore, live, conductorStore, tabsStore, draftsStore, ghStore,
    glStore, serversStore, awsStore, azureStore, claudeStore, codexStore, appSettings, features,
    modelCatalog, uiLayout, confirm, prompt, toast, projectId, project, isSessionWindow, activeRunId,
    engineOverride, modelOverride, loadedRunEngine, syncLoadedRunEngine, effectiveEngine, sessionDefaults,
    permissionMode, currentRunId, currentRun, currentIsSession, session, liveEntries,
    liveOutputLines, liveHasStream, currentStatus, currentSessionId, runLabel,
  }
}

export type RunSessionCore = ReturnType<typeof useRunSessionCore>
