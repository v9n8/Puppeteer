// The composer and everything that sends from it: run settings (model, Claude
// account, permission), the per-session draft with attachments, quotes, slash
// commands, @-mentions, and start / follow-up / steer / queue /
// cancel / engine handoff. Part of the RunView split (see useRunSession.ts).
import { ref, computed, onBeforeUnmount, onUnmounted, watch, nextTick } from 'vue'
import type { RunRecord, ProjectEntry } from '@/stores/runs'
import { invoke } from '@/lib/tauri'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { Cpu, Terminal } from 'lucide-vue-next'
import { useSelectionSurface } from '@/composables/useSelectionSurface'
import { transcriptMarkdownSources } from '@/lib/selectionSurface'
import {
  composeWithQuotes, entryOrTextQuote, fileQuote, quoteInput, type DraftQuote, type QuoteSource,
} from '@/lib/quote'
import type { QuoteAnchor } from '@/lib/quoteAnchor'
import { appendQuoteToDraft, composerWindowLabel, registerQuoteComposer } from '@/lib/quoteAppend'
import { emptyQueue, queuedPreview, type QueuedMessage } from '@/lib/messageQueue'
import { useMessageQueueStore } from '@/stores/messageQueue'
import { linkTargetRun, type LinkTarget } from '@/lib/sessionLinks'
import {
  latestChecklist, mergeChecklistIntoDraft, parseChecklistBlock, type TodoItem,
} from '@/lib/todoChecklist'
import { mergeContextModel } from '@/lib/contextLimits'
import type { TurnSide } from '@/lib/turnActions'
import { canForkRun } from '@/lib/runFork'
import {
  beginEdit, canEditResend, editDraftOf, endEdit, rewindErrorKey, rewindStep, type EditSession, type SentMessage,
} from '@/lib/editResend'
import type { ChatDraft } from '@/stores/chatDrafts'
import { createPathChecker, matchProjectFile } from '@/lib/fileLinks'
import { openFileWindow } from '@/lib/fileWindow'
import { FILE_MENTION_EVENT } from '@/lib/fileEvents'
import type { ImageAttachment } from '@/lib/streamEvents'
import {
  MODEL_OPTIONS, withCurrentModel, combinedModelOptions, engineForModel,
} from '@/lib/engineOptions'
import { inheritedModel } from '@/lib/composerChips'
import type { RunSessionCore } from './useRunSessionCore'
import type { RunConductor } from './useRunConductor'
import type { RunLog } from './useRunLog'
import type { RunHeaderInfo } from './useRunHeaderInfo'
import { useComposerChips } from './useComposerChips'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { quotaSwitchTargets, type QuotaSwitchAccount } from '@/lib/quotaSwitch'

interface PendingImage extends ImageAttachment {
  id: string
  url: string // data URL for the thumbnail preview
}

interface PendingFile {
  id: string
  name: string
  path: string
}

/** One message on its way out, whether typed in the composer or taken from the queue. */
interface OutgoingMessage {
  text: string
  images: ImageAttachment[]
  filePaths: string[]
  /** Sent ahead of the text as blockquotes, and saved as cards (UX-29). */
  quotes: DraftQuote[]
}

export function useRunComposer(core: RunSessionCore, conductor: RunConductor, log: RunLog, header: RunHeaderInfo) {
  const {
    t, runsStore, live, conductorStore, draftsStore, claudeStore, codexStore, modelCatalog, confirm,
    toast, projectId, project, engineOverride, modelOverride, loadedRunEngine, syncLoadedRunEngine,
    effectiveEngine, sessionDefaults, permissionMode, currentRunId, currentRun, currentIsSession, session,
    liveEntries, liveOutputLines, liveHasStream, currentStatus, currentSessionId, runLabel,
  } = core
  const {
    conductorMaxWorkers,
  } = conductor
  const {
    outputEl, historyEl, viewingLogRunId, viewingLog, historyEntries, historyHasStream,
    historyContextTokens, historyModel, isViewingHistory, sourceText, stickToBottom,
    scrollOutputToBottom, scrollMemory, bookmarkState, toggleBookmark, snapshotTurns, rewindTarget,
    clearHistoryView, loadRunLog, loadQuotes, noteSentQuotes, forkRun, onRewound, promptNumber,
  } = log
  const {
    captureContext,
  } = header
  // Permission mode / effort / style chips (UX-26); their picks join every launch below.
  const chips = useComposerChips(core)
  const { choose } = useConfirm()
  const { dismiss: dismissToast } = useToast()

  const runSettingsOpen = ref(false)
  const runSettingsRoot = ref<HTMLElement | null>(null)
  const engineBadgeLabel = computed(() => effectiveEngine.value === 'codex' ? 'Codex' : 'Claude')
  const engineBadgeIcon = computed(() => effectiveEngine.value === 'codex' ? Terminal : Cpu)
  const runSettingsTitle = computed(() => t('run.runSettingsTitle', { engine: engineBadgeLabel.value }))
  function toggleRunSettingsMenu() {
    runSettingsOpen.value = !runSettingsOpen.value
  }
  function closeRunSettingsMenu() {
    runSettingsOpen.value = false
  }
  function onRunSettingsPointerDown(e: MouseEvent) {
    if (!runSettingsOpen.value) return
    const target = e.target
    if (!(target instanceof Element)) return
    if (runSettingsRoot.value?.contains(target)) return
    if (target.closest('[data-app-select-dropdown]')) return
    closeRunSettingsMenu()
  }
  // One unified picker: Claude + Codex models in a single grouped list, so the
  // chosen model implies the engine and no separate engine selector is needed.
  // Option tables live in @/lib/engineOptions (shared with the remote controller):
  // Claude's is curated + annotated with the last validation sweep; Codex's is
  // whatever `codex debug models` last reported.
  //
  // The model that an empty (Default) pick resolves to: the one the run last ran
  // with (a resume keeps it), else the project's default, else the global one —
  // read reactively, so changing a default is reflected here at once.
  const defaultModel = computed(() => inheritedModel(currentRun.value, effectiveEngine.value, sessionDefaults.value))
  const modelOptions = computed(() => {
    let claude = modelCatalog.withValidation(MODEL_OPTIONS.claude)
    let codex = modelCatalog.codexOptions(MODEL_OPTIONS.codex)
    // Keep an off-table saved pick selectable (a dropped/aliased id) by appending
    // it to the group matching the loaded run's engine.
    const cur = modelOverride.value
    if (cur && !claude.some(o => o.value === cur) && !codex.some(o => o.value === cur)) {
      const eng = loadedRunEngine.value || currentRun.value?.engine
      if (eng === 'codex') codex = withCurrentModel(codex, cur)
      else claude = withCurrentModel(claude, cur)
    }
    const def = defaultModel.value
    return combinedModelOptions({
      claude,
      codex,
      defaultLabel: def ? t('run.modelDefaultWith', { model: def }) : t('run.modelDefault'),
      claudeLabel: 'Claude',
      codexLabel: 'Codex',
    })
  })
  // Picking a model drives the engine. Switching engine mid-conversation routes
  // through the handoff flow (which remembers the model to apply after the switch).
  const pendingEngineModel = ref<string | null>(null)
  function onModelSelect(value: string) {
    const nextEngine = engineForModel(value, modelOptions.value) // '' = Default → global
    const currentEngine = loadedRunEngine.value || currentRun.value?.engine || effectiveEngine.value
    const resolvedNext = nextEngine || sessionDefaults.value.engine
    const hasConversation =
      !!currentRunId.value &&
      currentStatus.value !== 'fetched' &&
      sourceText.value.trim().length > 0
    if (hasConversation && resolvedNext !== currentEngine) {
      pendingEngineModel.value = value
      closeRunSettingsMenu()
      handoffTarget.value = resolvedNext
      return
    }
    engineOverride.value = nextEngine
    modelOverride.value = value
  }

  // Context-window meter state for the focused run. Prefer the live session; for
  // a past run with no live session, use the figures reconstructed from its log.
  const contextTokens = computed(() => session.value?.contextTokens ?? historyContextTokens.value)
  const contextModel = computed(() =>
    // `system.init` drops the `[1m]` suffix; re-attach it from the composer pick so
    // 1M runs resolve to the 1M context limit rather than 200K.
    mergeContextModel(
      session.value?.model ?? historyModel.value ?? currentRun.value?.engine ?? null,
      modelOverride.value,
    ),
  )
  // Model shown on the composer's run-settings badge: the live session's actual
  // model when available, else the run's persisted model, else the pending pick
  // or the resolved default. Empty only when nothing is known yet.
  const composerModelLabel = computed(() =>
    session.value?.model
    || historyModel.value
    || modelOverride.value
    || currentRun.value?.model
    || defaultModel.value
    || '',
  )
  const contextRateLimit = computed(() => session.value?.rateLimit ?? null)

  function setLocalRunStatus(runId: string, status: RunRecord['status'], engine?: string) {
    const run = runsStore.runs.find(r => r.id === runId)
    if (run) {
      run.status = status
      if (engine) run.engine = engine
    }
  }

  // Whether the run-blocking guardrail for `engine` refuses a new turn right now.
  async function budgetIsOver(engine: string): Promise<boolean> {
    try {
      const status = await invoke<{ is_over: boolean }>('get_run_budget', {
        engine,
        runId: currentRunId.value,
        projectId: projectId.value,
      })
      return status.is_over
    } catch {
      return false // can't determine the verdict → don't block the user
    }
  }

  // Preflight the run-blocking guardrail for `engine` BEFORE the UI echoes the
  // user's message or flips a run into "running". Returns the override flag to
  // pass to the backend (false = under budget, true = user chose to proceed
  // anyway), or `null` when the user declines — in which case the caller must
  // abort without touching the timeline. When another account of the engine is
  // still under its quota, the dialog offers to move the session to it first
  // (UX-48); the send then goes out on that account.
  async function preflightBudget(engine: string): Promise<boolean | null> {
    if (!(await budgetIsOver(engine))) return false
    const runId = currentRunId.value
    const target = await quotaSwitchTarget(engine)
    if (runId && target) {
      const choice = await choose({
        title: t('run.budgetExceededTitle'),
        message: t('run.quotaSwitch.message', { account: target.label }),
        confirmLabel: t('run.quotaSwitch.continueOn', { account: target.label }),
        altLabel: t('run.quotaSwitch.stay'),
        variant: 'primary',
      })
      if (choice === 'alt') return true
      if (choice !== 'confirm' || !(await switchRunAccount(runId, engine, target.value))) return null
      // The guard now reads the new account.
      return preflightBudget(engine)
    }
    const proceed = await confirm({
      title: t('run.budgetExceededTitle'),
      message: t('run.budgetConfirmMessage'),
      confirmLabel: t('run.continue'),
      variant: 'primary',
    })
    return proceed ? true : null
  }

  async function handleStartRun() {
    if (!currentRunId.value) return
    const id = currentRunId.value
    const engine = effectiveEngine.value

    const override = await preflightBudget(engine)
    if (override === null) return // over budget and declined — start nothing

    // A never-started issue/PR run launches as a conductor on its default
    // issue/PR prompt (empty goal). A fresh session can't get here without a goal
    // — primaryDisabled blocks the empty Run.
    if (currentStatus.value === 'fetched') {
      await launchConductor(id, '', [], override)
      return
    }

    clearHistoryView()
    live.reset(id, projectId.value)
    syncLoadedRunEngine(engine)
    setLocalRunStatus(id, 'running', engine)
    await live.startListening(id, projectId.value)

    try {
      const { effort, style } = chips.chipOverrides(engine)
      await runsStore.startRun(
        id,
        engine,
        permissionMode.value || undefined,
        undefined,
        modelOverride.value || undefined,
        undefined,
        override,
        effort,
        style,
      )
    } catch (e) {
      live.setStatus(id, 'failed')
      setLocalRunStatus(id, 'failed')
      live.appendOutput(id, `Error: ${String(e)}`)
    }
  }

  // Create a standalone work session (not tied to a GitHub issue/PR) and focus
  // the composer so the user can type the first instruction right away.
  const creatingSession = ref(false)
  async function createSessionWithEngine(engine?: string) {
    if (creatingSession.value) return
    creatingSession.value = true
    try {
      const run = await runsStore.createSessionRun(projectId.value, engine)
      await runsStore.fetchRuns(projectId.value)
      await loadRunLog(run.id)
      focusComposer()
    } catch (e) {
      toast.error(t('run.createSessionFailed', { error: String(e) }))
    } finally {
      creatingSession.value = false
    }
  }

  // The single creation path: a fresh session is a pre-launch conductor (the
  // worker sidebar opens so the cap can be set); its first message becomes the
  // goal and launches it via conductorStore.start (see launchConductor).
  async function handleNewSession() {
    await createSessionWithEngine(engineOverride.value || undefined)
  }

  // Launch a fetched run (fresh session, issue/PR or engine handoff) as a
  // conductor with `goal` as its first message — the only first-run path, since
  // every session is a conductor. It then streams here like any session; the
  // worker sidebar follows once its tree is selected. Callers MUST preflight the
  // budget (see `preflightBudget`) — this echoes the goal immediately.
  async function launchConductor(
    runId: string,
    goal: string,
    images: ImageAttachment[] = [],
    overrideBudget = false,
    engineChoice?: string,
  ) {
    const engine = engineChoice || effectiveEngine.value
    // Before the switch below: a handoff launches a new run with this session's picks.
    const { effort, style } = chips.chipOverrides(engine)
    if (currentRunId.value) chips.carryPicks(currentRunId.value, runId)
    scrollMemory.flush()
    currentRunId.value = runId
    clearHistoryView()
    live.reset(runId, projectId.value)
    if (goal) live.pushUser(runId, projectId.value, goal, images)
    stickToBottom.value = true // re-pin: the user just sent a message
    scrollMemory.forget(runId)
    syncLoadedRunEngine(engine)
    setLocalRunStatus(runId, 'running', engine)
    await live.startListening(runId, projectId.value)
    try {
      await conductorStore.start(runId, goal, {
        maxWorkers: conductorMaxWorkers.value,
        engineOverride: engine,
        modelOverride: modelOverride.value || undefined,
        effortOverride: effort,
        styleOverride: style,
        permissionModeOverride: chips.modeValue.value,
        images,
        overrideBudget,
      })
    } catch (e) {
      // The launch never happened, so the DB row is still 'fetched'. Re-sync the
      // local status from the DB (not a guessed 'failed') so the next send takes
      // the launch path again instead of trying to resume a run that never ran.
      live.appendOutput(runId, `Error: ${String(e)}`)
      try {
        await runsStore.refreshMeta(projectId.value)
      } catch { /* keep the local row; fall back to 'fetched' below */ }
      const dbStatus = runsStore.runs.find((r) => r.id === runId)?.status ?? 'fetched'
      live.setStatus(runId, dbStatus)
      setLocalRunStatus(runId, dbStatus)
      return
    }
    // start_conductor marked role='conductor' and derived the title from the goal.
    // Refresh in place (no `loading` flash) so History shows both immediately.
    await runsStore.refreshMeta(projectId.value)
    void conductorStore.select(runId)
  }

  // ── Cross-engine handoff (Claude ↔ Codex) ────────────────────────────────
  const handoffTarget = ref<string | null>(null)
  const handingOff = ref(false)
  const startingEngineSession = ref(false)
  const engineSwitchBusy = computed(() => handingOff.value || startingEngineSession.value)

  // Dismiss the handoff prompt without switching (also drops any pending model).
  function closeHandoffDialog() {
    handoffTarget.value = null
    pendingEngineModel.value = null
  }

  async function doHandoff(targetEngine: string) {
    if (!currentRunId.value || engineSwitchBusy.value) return
    handingOff.value = true
    const sourceId = currentRunId.value
    const sourceEngine = loadedRunEngine.value || currentRun.value?.engine || 'claude'
    try {
      // Snapshot the transcript before any state reset wipes streamEntries.
      const transcript = sourceText.value
      // A live session can't be forked — stop it first, then continue on the new engine.
      if (currentStatus.value === 'running') {
        try { await runsStore.cancelRun(sourceId) } catch { /* best effort */ }
      }
      const override = await preflightBudget(targetEngine)
      if (override === null) return // over budget on the target engine — abort handoff start
      // Rust summarizes the transcript first (up to 90 s), hence "Summarizing context…".
      const { run, context_path, summary_lines } = await runsStore.createHandoffRun(sourceId, targetEngine, transcript)
      await runsStore.fetchRuns(projectId.value) // surface the new run in the sidebar
      syncLoadedRunEngine(targetEngine)
      // Carry the model the user picked when they triggered the switch.
      if (pendingEngineModel.value !== null) modelOverride.value = pendingEngineModel.value
      const seed = summary_lines
        ? t('run.handoffSeedSummary', { engine: sourceEngine, path: context_path, lines: summary_lines })
        : t('run.handoffSeed', { engine: sourceEngine, path: context_path })
      await launchConductor(run.id, seed, [], override, targetEngine)
    } catch (e) {
      toast.error(t('run.handoffFailed', { error: String(e) }))
    } finally {
      handingOff.value = false
      handoffTarget.value = null
      pendingEngineModel.value = null
    }
  }

  async function startNewEngineSession(targetEngine: string) {
    if (engineSwitchBusy.value) return
    startingEngineSession.value = true
    try {
      handoffTarget.value = null
      if (pendingEngineModel.value !== null) modelOverride.value = pendingEngineModel.value
      await createSessionWithEngine(targetEngine)
    } finally {
      startingEngineSession.value = false
      pendingEngineModel.value = null
    }
  }

  const followUpInput = ref('')
  // Quote cards above the composer (UX-29): part of the draft, sent ahead of the text.
  const draftQuotes = ref<DraftQuote[]>([])
  const sendingFollowUp = ref(false)
  // "Edit and resend" (UX-30): the sent message the composer holds, and the
  // draft set aside for it. `rewindingEdit`: its Send is rewinding (or the
  // Rewind dialog is open for it).
  const editing = ref<EditSession<ChatDraft> | null>(null)
  const editingMessage = computed(() => (editing.value && editing.value.runId === currentRunId.value ? editing.value : null))
  const rewindingEdit = ref(false)
  const savingAccount = ref(false)

  const defaultClaudeAccount = computed(() =>
    claudeStore.accounts.find((acc) => acc.is_default) ?? null,
  )
  const effectiveProjectClaudeAccountId = computed(() =>
    project.value?.claude_account_id || defaultClaudeAccount.value?.id || '',
  )
  const selectedClaudeAccountId = computed(() => {
    const run = currentRun.value
    if (!run) return ''
    // Empty = the Default tier (follows project/global). Only a run-level
    // override pins a concrete account for this session.
    return run.claude_account_id || ''
  })
  // Label for the Default account entry: resolves to the project/global default
  // account so the choice is never opaque. Falls back to "Global ~/.claude" only
  // when no default account is configured at all.
  const defaultClaudeAccountLabel = computed(() => {
    const acc = claudeStore.accounts.find((a) => a.id === effectiveProjectClaudeAccountId.value)
    return acc ? t('run.claudeAccountDefaultWith', { account: acc.label }) : t('run.globalClaudeAccount')
  })
  const showClaudeAccountSelect = computed(() =>
    !!currentRun.value
    && effectiveEngine.value === 'claude'
    && (claudeStore.accounts.length > 0 || !!currentRun.value.claude_account_id),
  )
  // The Claude account this session runs on (its own pick, else the
  // project/global default), for the account · model chip. '' when there is
  // no account to choose.
  const activeClaudeAccountLabel = computed(() => {
    if (!showClaudeAccountSelect.value) return ''
    const id = selectedClaudeAccountId.value || effectiveProjectClaudeAccountId.value
    return claudeStore.accounts.find((acc) => acc.id === id)?.label ?? ''
  })
  // Codex (UX-40): a run keeps the login it ran with, so there is no
  // "Default" tier; '' is the shared `~/.codex` login.
  const showCodexAccountSelect = computed(() =>
    !!currentRun.value
    && effectiveEngine.value === 'codex'
    && (codexStore.accounts.length > 0 || !!currentRun.value.codex_account_id),
  )
  const activeCodexAccountLabel = computed(() => {
    if (!showCodexAccountSelect.value) return ''
    const id = currentRun.value?.codex_account_id
    if (!id) return t('run.accountMenu.sharedCodexLogin')
    return codexStore.accounts.find((acc) => acc.id === id)?.label ?? ''
  })
  // The picked row of the Account tab, for either engine.
  const selectedAccountId = computed(() =>
    effectiveEngine.value === 'codex' ? currentRun.value?.codex_account_id || '' : selectedClaudeAccountId.value,
  )
  // The account · model chip's label, on the composer and the status bar.
  const accountModelLabel = computed(() =>
    [activeClaudeAccountLabel.value || activeCodexAccountLabel.value || engineBadgeLabel.value, composerModelLabel.value]
      .filter(Boolean)
      .join(' · '),
  )
  // The Account tab of the account · model menu (UX-47). `value` null: the one
  // login there is, shown but not a choice (Codex or Claude with no managed
  // account). `usageKey` names the usage row it shows, as the status bar's
  // usage rings key them.
  const accountChoices = computed(() => {
    type Choice = { value: string | null; label: string; detail?: string; usageKey: string | null }
    if (effectiveEngine.value === 'codex') {
      if (!showCodexAccountSelect.value) {
        return [{ value: null, label: t('run.accountMenu.sharedCodexLogin'), usageKey: 'codex' }] as Choice[]
      }
      const out: Choice[] = [{ value: '', label: t('run.accountMenu.sharedCodexLogin'), usageKey: 'codex' }]
      const currentId = currentRun.value?.codex_account_id
      if (currentId && !codexStore.accounts.some((acc) => acc.id === currentId)) {
        out.push({ value: currentId, label: t('run.missingCodexAccount'), usageKey: null })
      }
      for (const acc of codexStore.accounts) {
        out.push({ value: acc.id, label: acc.label, detail: acc.email || undefined, usageKey: `codex:${acc.id}` })
      }
      return out
    }
    if (!showClaudeAccountSelect.value) {
      return [{ value: null, label: t('run.globalClaudeAccount'), usageKey: 'claude' }] as Choice[]
    }
    // The Default tier first, so a per-session pick can be cleared back to
    // "follow project/global"; its account's figures show on that account's row.
    const out: Choice[] = [{ value: '', label: defaultClaudeAccountLabel.value, usageKey: null }]
    const currentId = currentRun.value?.claude_account_id
    if (currentId && !claudeStore.accounts.some((acc) => acc.id === currentId)) {
      out.push({ value: currentId, label: t('run.missingClaudeAccount'), usageKey: null })
    }
    for (const acc of claudeStore.accounts) {
      out.push({ value: acc.id, label: acc.label, detail: acc.email || undefined, usageKey: `claude:${acc.id}` })
    }
    return out
  })
  // Allowed while a turn runs: the switch is stored and the next turn resumes
  // under the new account (set_run_*_account copies nothing mid-turn).
  const accountSelectDisabled = computed(() => sendingFollowUp.value || savingAccount.value)

  async function handleAccountChange(value: string) {
    const run = currentRun.value
    if (!run) return
    const next = value.trim() || null
    const codex = effectiveEngine.value === 'codex'
    if (((codex ? run.codex_account_id : run.claude_account_id) ?? null) === next) return
    await switchRunAccount(run.id, codex ? 'codex' : 'claude', next ?? '')
  }

  // Put run `runId` on another `engine` account ('' = the Default tier for
  // Claude, the shared login for Codex). False when it did not change.
  async function switchRunAccount(runId: string, engine: string, value: string): Promise<boolean> {
    if (savingAccount.value) return false
    const codex = engine === 'codex'
    const keys = codex
      ? ['run.codexAccountUpdated', 'run.codexAccountUpdatedNextTurn', 'run.codexAccountUpdateFailed']
      : ['run.claudeAccountUpdated', 'run.claudeAccountUpdatedNextTurn', 'run.claudeAccountUpdateFailed']
    savingAccount.value = true
    try {
      await (codex ? runsStore.setRunCodexAccount(runId, value || null) : runsStore.setRunClaudeAccount(runId, value || null))
      const status = runId === currentRunId.value ? currentStatus.value : runsStore.runs.find((r) => r.id === runId)?.status
      toast.success(t(status === 'running' ? keys[1] : keys[0]))
      return true
    } catch (e) {
      toast.error(t(keys[2], { error: String(e) }))
      return false
    } finally {
      savingAccount.value = false
    }
  }

  // ── Quota reached: offer another account (UX-48) ──────────────────────────
  // The usage row of the account this session runs `engine` on, as the chip
  // shows it: its own pick, else the project/global default for Claude, the
  // shared login for Codex.
  function runUsageKey(engine: string): string {
    const run = currentRun.value
    if (engine === 'codex') return run?.codex_account_id ? `codex:${run.codex_account_id}` : 'codex'
    const id = run?.claude_account_id || effectiveProjectClaudeAccountId.value
    return id ? `claude:${id}` : 'claude'
  }

  // The least used account of `engine` still under its quota, to move this
  // session to; null when there is none. Only the run's own engine: the
  // account setters refuse another one (e.g. ahead of a handoff).
  async function quotaSwitchTarget(engine: string): Promise<QuotaSwitchAccount | null> {
    if (!currentRun.value || currentRun.value.engine !== engine) return null
    const accounts: QuotaSwitchAccount[] = engine === 'codex'
      ? [
          { value: '', label: t('run.accountMenu.sharedCodexLogin'), usageKey: 'codex' },
          ...codexStore.accounts.map((acc) => ({ value: acc.id, label: acc.label, usageKey: `codex:${acc.id}` })),
        ]
      : claudeStore.accounts.map((acc) => ({ value: acc.id, label: acc.label, usageKey: `claude:${acc.id}` }))
    const currentKey = runUsageKey(engine)
    const others = accounts.filter((acc) => acc.usageKey !== currentKey)
    if (!others.length) return null
    await Promise.all(others.map((acc) => chips.loadUsage(acc.usageKey)))
    const targets = quotaSwitchTargets(
      accounts, currentKey, chips.usageSnapshots.value, chips.usageThresholds.value, Date.now(),
    )
    return targets[0] ?? null
  }

  // ── Pasted / attached images in the composer ──────────────────────────────
  const pendingImages = ref<PendingImage[]>([])
  let imageSeq = 0

  const MAX_IMAGE_BYTES = 10 * 1024 * 1024 // ~10MB per image, matches engine limits

  // ── Dragged-in files referenced by path ───────────────────────────────────
  // Non-image files are attached by absolute path (not embedded); the agent opens
  // them via its Read/Edit tools. See SRS FR-002/FR-004.
  const pendingFiles = ref<PendingFile[]>([])
  let fileSeq = 0
  // Whether an OS file is being dragged over the window (drives the dropzone hint).
  const isDraggingFile = ref(false)

  // ── Draft persistence ─────────────────────────────────────────────────────
  // The composer's unsent draft (text, pasted images and attached files) is kept
  // PER SESSION, keyed by run id: switching between sessions — or leaving the
  // screen and coming back (the run reopens from the route / most-recent) —
  // restores exactly that session's draft instead of a shared, easily-clobbered
  // project-wide one. Mirrored to localStorage so it also survives app reloads.
  // `restoringDraft` guards the persist watcher while we swap a session's draft in.
  let restoringDraft = false

  /** Put `draft` into the composer without saving it as the session's draft. */
  function applyComposerDraft(draft: ChatDraft | null) {
    restoringDraft = true
    pendingImages.value = []
    pendingFiles.value = []
    followUpInput.value = draft?.text ?? ''
    draftQuotes.value = draft?.quotes ?? []
    for (const img of draft?.images ?? []) pushPendingImage(img.media_type, img.data)
    for (const f of draft?.files ?? []) addPendingFile(f.path)
    nextTick(() => { restoringDraft = false })
  }

  function composerDraft(): ChatDraft {
    return {
      text: followUpInput.value,
      images: pendingImages.value.map(({ media_type, data }) => ({ media_type, data })),
      files: pendingFiles.value.map(({ name, path }) => ({ name, path })),
      quotes: draftQuotes.value,
    }
  }

  function restoreDraftFor(runId: string | null) {
    // An edit in progress (UX-30) ends with its session; that session's draft
    // never left the store.
    editing.value = null
    applyComposerDraft(runId ? draftsStore.get(runId) : null)
  }

  // Load the focused session's draft whenever it changes (initial open, session
  // switch, new session). The persist watcher below then tracks edits for it.
  watch(currentRunId, (id) => restoreDraftFor(id), { immediate: true })

  // While an edit is in the composer the stored draft is the one set aside for
  // it, so a reload in the middle still finds that draft.
  watch(
    [followUpInput, pendingImages, pendingFiles, draftQuotes],
    () => {
      if (restoringDraft || !currentRunId.value || editingMessage.value) return
      draftsStore.set(currentRunId.value, composerDraft())
    },
    { deep: true },
  )

  // Selected text in the output (live or history) gets the shared selection
  // toolbar: quote it into this run's composer, translate it, copy it as the
  // markdown it was rendered from, or keep it as a note / todo filed under this run.
  const selectionSurface = useSelectionSurface({
    el: [outputEl, historyEl],
    allow: { quote: true, translate: true, copyMd: true, saveNote: true, saveTodo: true },
    markdownSource: () => {
      if (isViewingHistory.value) {
        return historyHasStream.value ? transcriptMarkdownSources(historyEntries.value) : [viewingLog.value]
      }
      return liveHasStream.value
        ? transcriptMarkdownSources(liveEntries.value)
        : [liveOutputLines.value.map(l => l.text).join('\n')]
    },
    // The composer's text is this run's draft (persisted by the draft watcher).
    // No target while a send is in flight: the send clears the composer when it
    // returns, which would drop a quote added meanwhile.
    quoteTarget: () => {
      const run = currentRun.value
      if (!run || sendingFollowUp.value) return null
      return {
        label: runLabel(run),
        // Anchored to the message it was taken from when the selection was in one.
        append: (text: string, anchor?: QuoteAnchor | null, note?: string) => {
          addDraftQuote(entryOrTextQuote(crypto.randomUUID(), text, run.id, anchor, note))
          focusComposer()
        },
      }
    },
    capture: () => ({
      context: captureContext(),
      noteTitle: currentRun.value?.title ?? project.value?.name ?? '',
    }),
    // "Rewind to here" under a turn: only for a stopped session, to a turn whose
    // snapshot is still kept. The dialog confirms before anything changes.
    rewind: () => {
      const run = currentRun.value
      if (!run || currentStatus.value === 'running' || sendingFollowUp.value) return null
      return {
        available: (turn: number) => snapshotTurns.value.has(turn),
        start: (turn: number) => {
          rewindTarget.value = { runId: run.id, turn }
        },
      }
    },
    // "Edit and resend" under a question: where Rewind takes the conversation
    // back (lib/editResend), to a message whose snapshot is still kept.
    editResend: () => {
      const run = currentRun.value
      const busy = { running: currentStatus.value === 'running', sending: sendingFollowUp.value || rewindingEdit.value }
      if (!run || !canEditResend(run, busy)) return null
      return {
        available: (turn: number) => snapshotTurns.value.has(turn),
        start: (turn: number, promptAt: number, sent: SentMessage) => startEditResend(run.id, turn, promptAt, sent),
      }
    },
    // "Fork from here" under an answer: a stopped session the engine can fork
    // (lib/runFork). The confirm says files are not touched.
    fork: () => {
      const run = currentRun.value
      if (!run || currentStatus.value === 'running' || sendingFollowUp.value || !canForkRun(run)) return null
      return { start: (promptAt: number) => void forkRun(run, promptAt) }
    },
    bookmarks: () => {
      const run = currentRun.value
      if (!run || !(isViewingHistory.value ? historyHasStream.value : liveHasStream.value)) return null
      return {
        marked: bookmarkState,
        toggle: (promptAt: number, side: TurnSide) => void toggleBookmark(run.id, promptAt, side),
      }
    },
  })

  // A quote sent from a file window (lib/quoteAppend) for the run on screen goes
  // into the composer, not the stored draft the watcher above would overwrite. No
  // focus: the user is still reading in that other window.
  function takeQuote(text: string, source: QuoteSource, note?: string) {
    if (!sendingFollowUp.value) {
      addDraftQuote(fileQuote(crypto.randomUUID(), text, source, note))
      return
    }
    // The send clears the composer when it returns: add the quote after that.
    const runId = currentRunId.value
    if (!runId) return
    const stop = watch(sendingFollowUp, (busy) => {
      if (busy) return
      stop()
      if (currentRunId.value === runId) addDraftQuote(fileQuote(crypto.randomUUID(), text, source, note))
      else appendQuoteToDraft(runId, text, source, note)
    })
  }

  function addDraftQuote(q: DraftQuote) {
    if (q.excerpt.trim()) draftQuotes.value = [...draftQuotes.value, q]
  }
  function removeDraftQuote(id: string) {
    draftQuotes.value = draftQuotes.value.filter((q) => q.id !== id)
  }
  function setDraftQuoteNote(id: string, note: string) {
    draftQuotes.value = draftQuotes.value.map((q) => (q.id === id ? { ...q, note } : q))
  }
  onBeforeUnmount(registerQuoteComposer({ runId: () => currentRunId.value, append: takeQuote }))

  // Extensions we treat as inline images (base64 + thumbnail). Everything else is
  // attached by path.
  const IMAGE_EXT_RE = /\.(png|jpe?g|gif|webp)$/i

  // Push a raw-base64 image onto the pending strip, rebuilding the data URL used
  // for the thumbnail preview. Shared by paste, file-picker and drag-drop paths.
  function pushPendingImage(media_type: string, data: string) {
    pendingImages.value.push({
      id: `img-${imageSeq++}`,
      media_type,
      data,
      url: `data:${media_type};base64,${data}`,
    })
  }

  // Read a File/Blob into a base64 attachment and add it to the pending strip.
  function addImageFile(file: File | null) {
    if (!file || !file.type.startsWith('image/')) return
    if (file.size > MAX_IMAGE_BYTES) {
      toast.error(t('run.imageTooLarge', { size: MAX_IMAGE_BYTES / 1024 / 1024 }))
      return
    }
    const reader = new FileReader()
    reader.onload = () => {
      const url = String(reader.result || '')
      const comma = url.indexOf(',')
      if (comma < 0) return
      pushPendingImage(file.type, url.slice(comma + 1))
    }
    reader.readAsDataURL(file)
  }

  // Handle a batch of absolute paths from a drag-drop (or the file picker): images
  // are read to base64 through the backend (frontend fs scope can't reach arbitrary
  // paths); other files are attached by reference. An image that can't be inlined
  // (e.g. over the size limit) falls back to a path reference so it isn't lost.
  async function addDroppedPaths(paths: string[]) {
    for (const path of paths) {
      if (IMAGE_EXT_RE.test(path)) {
        try {
          const img = await invoke<{ media_type: string; data: string }>('read_file_base64', { path })
          pushPendingImage(img.media_type, img.data)
        } catch (e) {
          toast.info(t('run.imageAttachedByPath', { error: String(e) }))
          addPendingFile(path)
        }
      } else {
        addPendingFile(path)
      }
    }
  }

  function addPendingFile(path: string) {
    if (pendingFiles.value.some((f) => f.path === path)) return // dedupe (BR-004)
    const name = path.split(/[/\\]/).pop() || path
    pendingFiles.value.push({ id: `file-${fileSeq++}`, name, path })
  }

  function removePendingFile(id: string) {
    pendingFiles.value = pendingFiles.value.filter((f) => f.id !== id)
  }

  // Attach files via the native picker (second entry point alongside drag-drop).
  // Returns absolute paths, consistent with the drag-drop flow.
  async function onPickFiles() {
    const selected = await openFileDialog({ multiple: true, directory: false })
    if (!selected) return
    const paths = Array.isArray(selected) ? selected : [selected]
    await addDroppedPaths(paths)
  }

  function takePendingFilePaths(): string[] {
    const paths = pendingFiles.value.map((f) => f.path)
    pendingFiles.value = []
    return paths
  }

  // Append dropped file references to the outgoing prompt as absolute paths so the
  // agent can Read/Edit them. Literal backticked paths (not `@mentions`) — the
  // sidecar stream mode doesn't expand `@`, and both engines accept absolute paths.
  function composePrompt(text: string, paths: string[]): string {
    if (!paths.length) return text
    const list = paths.map((p) => `- \`${p}\``).join('\n')
    const block = `${t('run.attachFileLabel')}\n${list}`
    return text ? `${text}\n\n${block}` : block
  }

  // Capture images pasted into the textarea (Cmd+V from a screenshot, etc.).
  function onComposerPaste(e: ClipboardEvent) {
    const items = e.clipboardData?.items
    if (!items) return
    let handled = false
    for (const item of items) {
      if (item.kind === 'file' && item.type.startsWith('image/')) {
        const file = item.getAsFile()
        if (file) { addImageFile(file); handled = true }
      }
    }
    // Only swallow the paste when it actually carried an image, so text pastes
    // (and mixed paste) still land in the textarea.
    if (handled) e.preventDefault()
  }

  function removePendingImage(id: string) {
    pendingImages.value = pendingImages.value.filter((img) => img.id !== id)
  }

  function takePendingImages(): ImageAttachment[] {
    const imgs = pendingImages.value.map(({ media_type, data }) => ({ media_type, data }))
    pendingImages.value = []
    return imgs
  }
  const chatEligible = computed(() => {
    if (!currentRunId.value) return false
    // Both Claude and Codex run through a sidecar now (streaming + permission +
    // multi-turn), so the composer is available for both engines.
    if (currentStatus.value === 'running') return true
    // Allow starting the run with a custom prompt when nothing has run yet.
    if (currentStatus.value === 'fetched') return true
    // Allow resume when this run captured a session id (thread id for Codex).
    if (!currentSessionId.value) return false
    return ['done', 'cancelled', 'failed'].includes(currentStatus.value)
  })

  const engineLabel = computed(() => loadedRunEngine.value || currentRun.value?.engine || effectiveEngine.value || t('run.engineFallback'))

  const chatPlaceholder = computed(() => {
    if (currentStatus.value === 'running') {
      return t('run.chatPlaceholderRunning', { engine: engineLabel.value })
    }
    if (currentStatus.value === 'fetched') {
      return currentIsSession.value
        ? t('run.chatPlaceholderFetchedSession')
        : t('run.chatPlaceholderFetchedIssue')
    }
    if (currentSessionId.value) {
      return t('run.chatPlaceholderResume', { engine: engineLabel.value })
    }
    return t('run.chatPlaceholderUnavailable')
  })

  const chatSendLabel = computed(() => {
    if (sendingFollowUp.value) return t('run.sendLabelSending')
    if (editingMessage.value) return t(editingMessage.value.rewound ? 'run.editResend.sendRewound' : 'run.editResend.send')
    if (currentStatus.value === 'fetched') return t('run.sendLabelRun')
    if (!chatEligible.value) return t('run.sendLabelRun')
    return t('run.sendLabelResume')
  })

  // Whether the composer footer should be shown at all — any loaded run, so the
  // engine/model/permission controls and the primary action live in one place.
  const composerVisible = computed(() => !!currentRunId.value)

  // The primary footer button is disabled when there's nothing to do. A default
  // "Run" (fetched record, or a finished run we can't resume) needs no input;
  // sending a follow-up / resuming does.
  const primaryDisabled = computed(() => {
    if (sendingFollowUp.value || rewindingEdit.value) return true
    if (editingMessage.value) return !hasComposerInput()
    // A fresh session launches as a conductor, which needs a goal to start.
    if (currentStatus.value === 'fetched' && currentRun.value?.run_type === 'session') {
      return !hasComposerInput()
    }
    if (currentStatus.value === 'fetched' || !chatEligible.value) return false
    return !hasComposerInput()
  })

  function hasComposerInput(): boolean {
    return !!followUpInput.value.trim() || !!pendingImages.value.length || !!pendingFiles.value.length
      || !!draftQuotes.value.length
  }

  // One entry point for the footer's primary button: cancel while running is a
  // separate button, so here we either queue / send a typed message / resume, or
  // kick off the default-prompt run when nothing was typed.
  function handlePrimaryAction() {
    if (editingMessage.value) { void handleSendFollowUp(); return }
    if (currentStatus.value === 'running') { handleQueue(); return }
    if (!hasComposerInput() && (currentStatus.value === 'fetched' || !chatEligible.value)) {
      handleStartRun()
      return
    }
    handleSendFollowUp()
  }

  // Send a slash command (e.g. /compact, /clear) as a message. Routes through the
  // normal primary action so it respects running / resume / start state.
  function sendSlashCommand(cmd: string) {
    followUpInput.value = cmd
    handlePrimaryAction()
  }

  // ── Composer auto-resize / expand ─────────────────────────────────────────
  // The composer grows with its content (great for pasted long text) up to a
  // cap, then scrolls internally. "Expanded" raises that cap to most of the
  // viewport so long text is comfortable to read and edit.
  const composerExpanded = ref(false)

  function autoResizeComposer() {
    const el = composerEl.value
    if (!el) return
    // Expanded: pin the textarea to a large fixed height so "maximize" is
    // immediately visible even when there's little/no content to grow into.
    if (composerExpanded.value) {
      const expandedPx = Math.round(window.innerHeight * 0.6)
      el.style.height = `${expandedPx}px`
      el.style.overflowY = 'auto'
      return
    }
    // Collapsed: grow with content up to a cap, then scroll internally.
    const maxPx = 180
    el.style.height = 'auto'
    const next = Math.min(el.scrollHeight, maxPx)
    el.style.height = `${next}px`
    el.style.overflowY = el.scrollHeight > maxPx ? 'auto' : 'hidden'
  }

  function toggleComposerExpand() {
    composerExpanded.value = !composerExpanded.value
    nextTick(() => {
      autoResizeComposer()
      composerEl.value?.focus()
    })
  }

  // Reliably move focus into the composer. A single `nextTick` is not enough when
  // the composer is mounting for the first time (e.g. right after a new session is
  // created, `currentRunId` also drives a concurrent `router.replace`): the
  // textarea may not be in the DOM yet, or focus gets stolen as the route settles.
  // Retry across a few animation frames until it takes.
  function focusComposer(attempts = 8) {
    nextTick(() => {
      const el = composerEl.value
      if (el) {
        el.focus({ preventScroll: true })
        if (document.activeElement === el) return
      }
      if (attempts > 0) requestAnimationFrame(() => focusComposer(attempts - 1))
    })
  }

  // Resize on every value change: typing, paste, programmatic edits
  // (slash/mention insert), and clearing after a send.
  watch(followUpInput, () => autoResizeComposer(), { flush: 'post' })

  // ── @-mention file/folder autocomplete ───────────────────────────────────
  const composerEl = ref<HTMLTextAreaElement | null>(null)
  const projectFiles = ref<ProjectEntry[]>([])
  const projectFilesPath = ref<string | null>(null)
  // Bumped whenever the list changes. The log's v-memo keys on it, so finalized
  // messages re-run file-link detection when the list arrives late or grows.
  const fileListVersion = ref(0)
  const mentionOpen = ref(false)
  const mentionQuery = ref('')
  const mentionStart = ref(0) // index of the `@` in followUpInput
  const mentionIndex = ref(0)
  const MENTION_LIMIT = 50

  // ── Slash-command palette (Claude advertises commands on system.init) ─────
  // Prefer the live session's list; when viewing a finished run from disk the
  // live session is gone, so fall back to the commands captured in the history.
  const slashCommands = computed(() => {
    const liveCmds = session.value?.slashCommands
    if (liveCmds && liveCmds.length) return liveCmds
    for (let i = historyEntries.value.length - 1; i >= 0; i--) {
      const e = historyEntries.value[i]
      if (e.kind === 'system' && e.slashCommands?.length) return e.slashCommands
    }
    // Brand-new session (no init event yet): fall back to the last commands this
    // engine advertised in any prior session.
    return live.cachedSlashCommands(effectiveEngine.value)
  })
  const slashOpen = ref(false)
  const slashQuery = ref('')
  const slashIndex = ref(0)
  const slashItems = computed(() => {
    const q = slashQuery.value.toLowerCase()
    return slashCommands.value.filter((c) => c.toLowerCase().startsWith(q)).slice(0, MENTION_LIMIT)
  })

  // Open the palette while the whole composer is a bare `/command` token.
  function detectSlash() {
    // Read straight from the DOM element rather than the v-model ref: while an IME
    // (e.g. Vietnamese Telex) is composing, Vue defers updating `followUpInput`
    // until `compositionend`, so the ref is stale on every `input` event. The
    // element's own `.value` is always current. Fall back to the ref pre-mount.
    const text = composerEl.value?.value ?? followUpInput.value
    const m = /^\/([a-zA-Z0-9_-]*)$/.exec(text)
    if (!m || !slashCommands.value.length) { slashOpen.value = false; return }
    slashQuery.value = m[1]
    slashIndex.value = 0
    slashOpen.value = true
  }

  function selectSlash(cmd: string) {
    followUpInput.value = `/${cmd}`
    slashOpen.value = false
    nextTick(() => {
      const el = composerEl.value
      if (el) { el.focus(); const pos = followUpInput.value.length; el.setSelectionRange(pos, pos) }
    })
  }

  function onComposerInput() {
    detectMention()
    detectSlash()
  }

  async function ensureProjectFiles(force = false) {
    const path = project.value?.path
    if (!path) return
    // Skip the walk only when the cache is warm AND no refresh was requested.
    // `force` is passed when a new @-mention session starts so newly-added files
    // (e.g. a file just copied into the folder) show up without a reload.
    if (!force && projectFilesPath.value === path && projectFiles.value.length) return
    let next: ProjectEntry[] = []
    let ok = false
    try {
      next = await runsStore.listProjectFiles(path)
      ok = true
    } catch { /* treated as an empty list */ }
    // The usual reload after a turn finds the same files: keep the list as is,
    // so the whole log isn't re-rendered for nothing.
    const prev = projectFiles.value
    const same = projectFilesPath.value === path && prev.length === next.length &&
      prev.every((e, i) => e.path === next[i].path && e.is_dir === next[i].is_dir)
    if (ok) projectFilesPath.value = path
    if (same) return
    projectFiles.value = next
    fileListVersion.value++
  }

  // Whether bare paths in the log's prose and prompts name real files, for those
  // the project list doesn't know (outside the project, ignored, new).
  const pathChecker = createPathChecker((paths) => {
    const projectPath = project.value?.path
    return projectPath ? runsStore.statPaths(projectPath, paths) : Promise.resolve(paths.map(() => false))
  })
  watch(() => project.value?.path, () => pathChecker.clear())

  // A turn that just ended may have created files: reload so they link, and
  // forget "no such file" answers given while it ran.
  watch([currentRunId, currentStatus], ([id, status], [prevId, prevStatus]) => {
    if (id === prevId && prevStatus === 'running' && status !== 'running') {
      pathChecker.clear()
      ensureProjectFiles(true)
    }
  })

  // ── Opening a file ────────────────────────────────────────────────────────
  // Every file opens in its own OS window (FileViewerWindow), never in a modal
  // over the conversation: reading a file is something you do ALONGSIDE the run,
  // and a modal covered the very output that made you click it. One window per
  // file, so several can sit side by side; reopening the same file focuses the
  // window already showing it.

  // Set of known project file paths, for resolving inline-code mentions.
  const projectFileSet = computed(() => new Set(projectFiles.value.map(e => !e.is_dir && e.path).filter(Boolean) as string[]))

  // Resolve a raw inline-code token (e.g. `src/foo.ts` or just `foo.ts`) to a
  // project file path, or null when it doesn't name a real file.
  function fileMatcher(raw: string): string | null {
    return matchProjectFile(raw, projectFileSet.value, project.value?.path)
  }

  /** Whether `path` (absolute, or relative to the project) is a file the viewer can open. */
  async function fileExists(path: string): Promise<boolean> {
    const projectPath = project.value?.path
    if (!projectPath || !path) return false
    try {
      const [hit] = await runsStore.statPaths(projectPath, [path])
      return !!hit
    } catch {
      return false
    }
  }

  /** Show a file (optionally scrolled to `line`) in its own window. */
  function openFile(path: string, line?: number | null) {
    const projectPath = project.value?.path
    if (!projectPath || !path) return
    openFileWindow(projectPath, path, line ?? null, currentRunId.value)
  }

  // Score an entry against the query. Higher = more relevant; -1 = no match.
  // Order of preference: exact basename > basename prefix > basename substring >
  // path substring > fuzzy subsequence over the path. This is what lets the file
  // you're actually typing surface ahead of alphabetically-earlier paths that
  // merely contain the same characters (the old code just took the first 50 in
  // alphabetical order, so deep/late matches never showed up).
  function scoreMention(path: string, q: string): number {
    const lp = path.toLowerCase()
    const slash = lp.lastIndexOf('/')
    const base = slash >= 0 ? lp.slice(slash + 1) : lp
    if (base === q) return 1000
    if (base.startsWith(q)) return 900 - base.length
    const bi = base.indexOf(q)
    if (bi >= 0) return 700 - bi - base.length * 0.01
    const pi = lp.indexOf(q)
    if (pi >= 0) return 500 - pi - lp.length * 0.01
    // Fuzzy: every query char appears in order somewhere in the path.
    let qi = 0
    for (let i = 0; i < lp.length && qi < q.length; i++) {
      if (lp[i] === q[qi]) qi++
    }
    if (qi === q.length) return 200 - lp.length * 0.01
    return -1
  }

  const mentionItems = computed<ProjectEntry[]>(() => {
    const q = mentionQuery.value.toLowerCase()
    const src = projectFiles.value
    if (!q) return src.slice(0, MENTION_LIMIT)
    const scored: Array<{ e: ProjectEntry; s: number }> = []
    for (const e of src) {
      const s = scoreMention(e.path, q)
      if (s >= 0) scored.push({ e, s })
    }
    // Highest score first; ties keep the (alphabetical) source order.
    scored.sort((a, b) => b.s - a.s)
    return scored.slice(0, MENTION_LIMIT).map((x) => x.e)
  })

  // Detect an active `@token` immediately before the caret (no whitespace inside).
  function detectMention() {
    const el = composerEl.value
    if (!el) { mentionOpen.value = false; return }
    const caret = el.selectionStart ?? 0
    // Read from the DOM element, not the v-model ref: during IME composition Vue
    // keeps `followUpInput` stale until `compositionend`, which would make the
    // query empty and surface the first-50 (alphabetical) files instead.
    const text = el.value
    let i = caret - 1
    while (i >= 0) {
      const ch = text[i]
      if (ch === '@') break
      if (ch === ' ' || ch === '\n' || ch === '\t') { mentionOpen.value = false; return }
      i--
    }
    if (i < 0 || text[i] !== '@') { mentionOpen.value = false; return }
    // `@` must start a token (preceded by whitespace or be at the very start).
    const prev = i > 0 ? text[i - 1] : ''
    if (prev && !/\s/.test(prev)) { mentionOpen.value = false; return }
    const wasOpen = mentionOpen.value
    mentionStart.value = i
    mentionQuery.value = text.slice(i + 1, caret)
    mentionIndex.value = 0
    mentionOpen.value = true
    // Refresh the file list when a mention session first opens (not on every
    // keystroke) so files added to the folder since the last session appear.
    ensureProjectFiles(!wasOpen)
  }

  function selectMention(entry: ProjectEntry) {
    const el = composerEl.value
    const caret = el?.selectionStart ?? followUpInput.value.length
    const text = followUpInput.value
    const before = text.slice(0, mentionStart.value)
    const after = text.slice(caret)
    const insert = `@${entry.path}${entry.is_dir ? '/' : ''} `
    followUpInput.value = before + insert + after
    mentionOpen.value = false
    nextTick(() => {
      const pos = before.length + insert.length
      if (el) { el.focus(); el.setSelectionRange(pos, pos) }
    })
  }

  // Insert a file mention (`@path`) into the composer. Reached from the shared
  // file menu — the explorer's right-click AND a file window's ⋯ menu — which
  // broadcasts it, because the composer only exists here in the main window.
  // Inserts at the caret when the composer is focused, otherwise appends.
  function mentionFileInComposer(path: string) {
    const el = composerEl.value
    const cur = followUpInput.value
    const focused = el && document.activeElement === el
    const caret = focused ? (el!.selectionStart ?? cur.length) : cur.length
    const before = cur.slice(0, caret)
    const after = cur.slice(caret)
    const sep = before.length && !/\s$/.test(before) ? ' ' : ''
    const insert = `${sep}@${path} `
    followUpInput.value = before + insert + after
    mentionOpen.value = false
    nextTick(() => {
      const pos = (before + insert).length
      if (el) { el.focus(); el.setSelectionRange(pos, pos) }
    })
  }

  // A reference from CapturePicker (Slack / web / note / todo) goes in at the
  // caret, appending when the composer isn't focused — same contract as
  // `mentionFileInComposer`. Deliberately does NOT send: the user reviews/edits first.
  function insertCaptureRef(text: string) {
    const el = composerEl.value
    const cur = followUpInput.value
    const focused = el && document.activeElement === el
    const caret = focused ? (el!.selectionStart ?? cur.length) : cur.length
    const before = cur.slice(0, caret)
    const after = cur.slice(caret)
    // Keep an existing draft readable by dropping the reference onto its own line.
    const sep = before.length && !/\n$/.test(before) ? '\n' : ''
    const insert = `${sep}${text}`
    followUpInput.value = before + insert + after
    nextTick(() => {
      const pos = (before + insert).length
      if (el) { el.focus(); el.setSelectionRange(pos, pos) }
    })
  }

  // The agent's checklist, editable from the composer: the list already in the
  // draft (edited, not sent yet), else the newest one in the log on screen. Null
  // hides the button (no TodoWrite yet, or an engine that never writes one).
  const composerChecklist = computed(() => {
    const inDraft = parseChecklistBlock(followUpInput.value)
    if (inDraft) return inDraft
    if (isViewingHistory.value) return historyHasStream.value ? latestChecklist(historyEntries.value) : null
    return liveHasStream.value ? latestChecklist(liveEntries.value) : null
  })

  function applyChecklist(todos: TodoItem[]) {
    followUpInput.value = mergeChecklistIntoDraft(followUpInput.value, todos)
    focusComposer()
  }

  // A Slack thread / note / todo picked into this session's composer gets linked
  // to the session (best-effort: the reference is already inserted, a failure only
  // warns). Every session is a run from the moment it is created, so there is
  // always an id here.
  function linkPicked(target: LinkTarget) {
    const runId = currentRunId.value
    if (!runId) return
    linkTargetRun(target, runId, 'picker').catch((e) =>
      toast.error(t('sessionLink.pickerLinkFailed', { error: String(e) })),
    )
  }

  function onComposerKeydown(e: KeyboardEvent) {
    if (slashOpen.value && slashItems.value.length) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        slashIndex.value = (slashIndex.value + 1) % slashItems.value.length
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        slashIndex.value = (slashIndex.value - 1 + slashItems.value.length) % slashItems.value.length
        return
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault()
        selectSlash(slashItems.value[slashIndex.value])
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        slashOpen.value = false
        return
      }
    }
    if (mentionOpen.value && mentionItems.value.length) {
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        mentionIndex.value = (mentionIndex.value + 1) % mentionItems.value.length
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        mentionIndex.value = (mentionIndex.value - 1 + mentionItems.value.length) % mentionItems.value.length
        return
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault()
        selectMention(mentionItems.value[mentionIndex.value])
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        mentionOpen.value = false
        return
      }
    }
    // Enter (no Shift) sends, or queues while a turn runs; Shift+Enter inserts a newline.
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      if (currentStatus.value === 'running' && !editingMessage.value) handleQueue()
      else handleSendFollowUp()
    }
  }

  // `steer`: add the message to the turn in flight (sessions.steer) instead of
  // sending a follow-up. A turn that ended meanwhile gets a plain follow-up.
  async function handleSendFollowUp(steer = false) {
    if (editingMessage.value) {
      await sendEditResend(editingMessage.value)
      return
    }
    mentionOpen.value = false
    const text = followUpInput.value.trim()
    if (!hasComposerInput() || !currentRunId.value || sendingFollowUp.value) return
    const quotes = draftQuotes.value
    const result = await sendMessage(
      currentRunId.value,
      () => ({ text, images: takePendingImages(), filePaths: takePendingFilePaths(), quotes }),
      steer,
    )
    if (result === 'sent') {
      followUpInput.value = ''
      // Quotes added while it was sending stay for the next message.
      const sentIds = new Set(quotes.map((q) => q.id))
      draftQuotes.value = draftQuotes.value.filter((q) => !sentIds.has(q.id))
    }
  }

  // ── Edit and resend (UX-30) ───────────────────────────────────────────────
  // The message's text, quotes (as cards again) and images go into the
  // composer; the draft that was there waits in the store. Send rewinds to just
  // before the message, as "Rewind to here" does (asking first when files would
  // be restored), then sends the edit as a follow-up. A failed or declined
  // rewind sends nothing and keeps the edit; Cancel puts the old draft back.
  function startEditResend(runId: string, turn: number, promptAt: number, sent: SentMessage) {
    if (currentRunId.value !== runId || sendingFollowUp.value || rewindingEdit.value) return
    if (!editingMessage.value) draftsStore.set(runId, composerDraft())
    editing.value = beginEdit(editing.value, { runId, turn, number: promptNumber(promptAt) }, composerDraft())
    const edit = editDraftOf(sent, () => crypto.randomUUID())
    applyComposerDraft({ text: edit.text, images: edit.images, files: [], quotes: edit.quotes })
    mentionOpen.value = false
    slashOpen.value = false
    focusComposer()
  }

  function cancelEditResend() {
    const edit = editingMessage.value
    if (!edit || sendingFollowUp.value || rewindingEdit.value) return
    editing.value = null
    applyComposerDraft(endEdit(edit))
  }

  function rewindErrorText(e: unknown): string {
    const key = rewindErrorKey(e)
    return key ? t(key) : String(e)
  }

  // True once the conversation went back to just before the message.
  async function rewindForEdit(runId: string, turn: number): Promise<boolean> {
    try {
      const preview = await runsStore.previewRewind(runId, turn)
      const step = rewindStep(preview)
      if (step === 'refuse') {
        const reason = t(`run.rewind.reason.${preview.conversation_reason ?? 'engine'}`)
        toast.error(t('run.editResend.cannotRewind', { reason }))
        return false
      }
      if (step === 'confirm') {
        return await new Promise<boolean>((settle) => {
          rewindTarget.value = { runId, turn, editResend: true, settle }
        })
      }
      const outcome = await runsStore.rewindRun(runId, turn, preview.fingerprint)
      let rewound = false
      await onRewound(outcome, { runId, turn, editResend: true, settle: (ok) => { rewound = ok } })
      return rewound
    } catch (e) {
      toast.error(rewindErrorText(e))
      return false
    }
  }

  async function sendEditResend(edit: EditSession<ChatDraft>) {
    mentionOpen.value = false
    if (!hasComposerInput() || sendingFollowUp.value || rewindingEdit.value) return
    const runId = edit.runId
    if (!edit.rewound) {
      rewindingEdit.value = true
      let rewound = false
      try {
        rewound = await rewindForEdit(runId, edit.turn)
      } finally {
        rewindingEdit.value = false
      }
      if (!rewound || editing.value !== edit) return
      // Not rewound again if the send below fails or is declined.
      edit.rewound = true
    }
    if (currentRunId.value !== runId) return
    const text = followUpInput.value.trim()
    const quotes = draftQuotes.value
    const result = await sendMessage(
      runId,
      () => ({ text, images: takePendingImages(), filePaths: takePendingFilePaths(), quotes }),
    )
    if (result === 'sent' && editing.value === edit) {
      editing.value = null
      applyComposerDraft(endEdit(edit))
    }
  }

  // Send one message to the run on screen: the first message of a fetched run,
  // a follow-up (resuming a finished run), or a steer. `take` hands over the
  // message only once the budget is confirmed, so on a decline it stays where
  // it was. 'failed' means it may already be echoed in the timeline.
  async function sendMessage(
    id: string,
    take: () => OutgoingMessage,
    steer = false,
  ): Promise<'sent' | 'declined' | 'failed'> {
    const isFetched = currentStatus.value === 'fetched'
    const wasRunning = currentStatus.value === 'running'
    const runEngine =
      (!isFetched && (loadedRunEngine.value || currentRun.value?.engine)) || effectiveEngine.value
    sendingFollowUp.value = true
    try {
      // Always confirm the budget BEFORE echoing anything into the timeline. On
      // decline nothing is pushed and no "thinking" indicator appears.
      const override = await preflightBudget(runEngine)
      if (override === null) return 'declined'

      const { text, images, filePaths, quotes } = take()
      // The quotes go to the model as blockquotes ahead of the text, as they did
      // when they were `> ` text in the draft.
      const prompt = composePrompt(composeWithQuotes(quotes, text), filePaths)

      if (isFetched) {
        // First message of a fetched run (fresh session or issue/PR): it becomes
        // the conductor goal.
        await launchConductor(id, prompt, images, override)
        return 'sent'
      }

      // This run becomes the live conversation. If we were viewing its on-disk
      // history, seed the live session with that transcript so prior context
      // stays visible, then leave history mode.
      const s = live.ensure(id, projectId.value)
      if (s.entries.length === 0 && viewingLogRunId.value === id && historyHasStream.value) {
        s.entries.push(...historyEntries.value)
        s.hasStreamEvents = true
        for (const e of historyEntries.value) {
          if (e.kind === 'system' && e.sessionId) s.sessionId = e.sessionId
          if (e.kind === 'system' && e.slashCommands?.length) s.slashCommands = e.slashCommands
        }
      }
      clearHistoryView()

      // Show the user message right away in the timeline (running / resume paths).
      const sent = live.pushUser(id, projectId.value, prompt, images)
      // A steer starts no turn of its own, so its quotes stay plain text.
      const quoteSet = quotes.length && !(steer && wasRunning) ? crypto.randomUUID() : null
      if (quoteSet && sent.kind === 'user') {
        sent.quoteSet = quoteSet
        noteSentQuotes(id, quoteSet, quotes)
      }
      stickToBottom.value = true // re-pin: the user just sent a message
      scrollMemory.forget(id)
      scrollOutputToBottom()

      if (!wasRunning) {
        live.setStatus(id, 'running')
        setLocalRunStatus(id, 'running')
        await live.startListening(id, projectId.value)
        const { effort, style } = chips.chipOverrides(runEngine)
        await runsStore.resumeRun(id, permissionMode.value || undefined, modelOverride.value || undefined, override, effort, style)
      }

      if (steer && wasRunning) {
        // No turn of its own, so no snapshot and no "Rewind to here".
        await runsStore.steerRun(id, prompt, images, override)
      } else {
        const turn = await runsStore.sendUserMessage(
          id, prompt, images, override, quoteSet ? { set: quoteSet, items: quotes.map(quoteInput) } : undefined,
        )
        // The snapshot turn it was logged under: "Rewind to here" finds it by this.
        if (turn !== null && sent.kind === 'user') sent.turn = turn
        if (quoteSet) void loadQuotes(id)
      }
      return 'sent'
    } catch (e) {
      live.appendOutput(id, t('run.sendFailed', { error: String(e) }))
      return 'failed'
    } finally {
      sendingFollowUp.value = false
    }
  }

  // ── Message queue (UX-25) ─────────────────────────────────────────────────
  // While a turn runs, Queue (and Enter) parks the whole draft as a chip above
  // the composer instead of sending it. When the turn ends well, the first chip
  // goes out as a normal follow-up, one per turn end; a failed or cancelled
  // turn, or a blocked budget, keeps them all. Rules: lib/messageQueue; storage:
  // stores/messageQueue.
  //
  // Only the window that takes this run's quotes sends from its queue (the
  // session pop-out when open, else the main window), and only while it shows
  // the run, because the send uses this composer's model / permission picks. A
  // turn that ends while the run is not on screen there leaves the chips for
  // the user to send. A remote controller doesn't see or drain this queue.
  const queueStore = useMessageQueueStore()
  const runQueue = computed(() => (currentRunId.value ? queueStore.get(currentRunId.value) : emptyQueue()))
  const queuedMessages = computed(() =>
    runQueue.value.items.map((item) => ({
      id: item.id,
      text: item.draft.text,
      label: queuedPreview(item),
      attachments: item.draft.images.length + item.draft.files.length,
    })),
  )
  // Why the chips are still waiting, shown above them once no turn is running.
  const queueNotice = computed(() => {
    if (!runQueue.value.items.length || currentStatus.value === 'running' || sendingFollowUp.value) return ''
    const hold = runQueue.value.hold
    if (hold === 'failed') return t('run.queueHeldFailed')
    if (hold === 'cancelled') return t('run.queueHeldCancelled')
    if (hold === 'budget') return t('run.queueHeldBudget')
    return t('run.queueWaiting')
  })

  function handleQueue() {
    mentionOpen.value = false
    slashOpen.value = false
    const id = currentRunId.value
    // An edit is sent by rewinding first, never queued behind a running turn.
    if (!id || sendingFollowUp.value || editingMessage.value) return
    const draft = {
      text: followUpInput.value,
      images: pendingImages.value.map(({ media_type, data }) => ({ media_type, data })),
      files: pendingFiles.value.map(({ name, path }) => ({ name, path })),
      quotes: draftQuotes.value,
    }
    if (!draft.text.trim() && !draft.images.length && !draft.files.length && !draft.quotes.length) return
    queueStore.enqueue(id, draft)
    followUpInput.value = ''
    pendingImages.value = []
    pendingFiles.value = []
    draftQuotes.value = []
  }

  function outgoingOf(item: QueuedMessage): OutgoingMessage {
    return {
      text: item.draft.text.trim(),
      images: item.draft.images,
      filePaths: item.draft.files.map((f) => f.path),
      quotes: item.draft.quotes ?? [],
    }
  }

  async function ownsQueue(runId: string): Promise<boolean> {
    try {
      return getCurrentWebviewWindow().label === (await composerWindowLabel(runId))
    } catch {
      return true // outside the Tauri shell: this is the only window
    }
  }

  async function drainQueue(runId: string, status: string) {
    if (!queueStore.get(runId).items.length || !(await ownsQueue(runId))) return
    const engine = loadedRunEngine.value || currentRun.value?.engine || effectiveEngine.value
    const overBudget = status === 'done' && (await budgetIsOver(engine))
    if (currentRunId.value !== runId) return // switched away meanwhile: the chips wait
    if (editingQueuedId.value) saveQueuedEdit() // send what the chip shows, not the old text
    const item = queueStore.turnEnded(runId, { status, overBudget })
    if (!item) return
    // Another send (typed, remote, a conductor wake) already started the next
    // turn: this one waits for that turn to end.
    if (sendingFollowUp.value || currentStatus.value === 'running') {
      queueStore.requeue(runId, item, null)
      return
    }
    const result = await sendMessage(runId, () => outgoingOf(item))
    if (result === 'declined') queueStore.requeue(runId, item, 'budget')
    else if (result === 'failed') queueStore.requeue(runId, item, 'failed')
  }
  onUnmounted(live.onAnyDone((runId, _projectId, status) => {
    if (runId !== currentRunId.value) return
    void drainQueue(runId, status)
    void offerQuotaSwitch(runId, status)
  }))

  // A turn of the run on screen ended with its account over the quota: the
  // turn tipped it over (`run:budget_exceeded`), or the quota stop cancelled
  // the session (UX-14). Offer to move the session to another account and send
  // the message the quota holds in the queue (UX-48). Only the window that
  // drains the queue asks, so a pop-out doesn't ask twice.
  let quotaToastId: number | null = null
  async function offerQuotaSwitch(runId: string, status: string) {
    if (status !== 'done' && status !== 'cancelled') return
    if (!(await ownsQueue(runId))) return
    const engine = loadedRunEngine.value || currentRun.value?.engine || effectiveEngine.value
    if (!(await budgetIsOver(engine))) return
    const target = await quotaSwitchTarget(engine)
    if (!target || currentRunId.value !== runId) return
    if (quotaToastId !== null) dismissToast(quotaToastId)
    const waiting = queueStore.get(runId).items.length > 0
    quotaToastId = toast.info(t('run.quotaSwitch.toast'), 15000, {
      label: waiting
        ? t('run.quotaSwitch.continueOn', { account: target.label })
        : t('run.quotaSwitch.switchTo', { account: target.label }),
      run: () => void switchAndContinue(runId, engine, target),
    })
  }

  async function switchAndContinue(runId: string, engine: string, target: QuotaSwitchAccount) {
    if (!(await switchRunAccount(runId, engine, target.value))) return
    // While a turn still runs, drainQueue sends it once that turn ends.
    const first = queueStore.get(runId).items[0]
    if (first && currentRunId.value === runId && currentStatus.value !== 'running') await sendQueuedNow(first.id)
  }

  // A chip's Send: now, as a follow-up. While a turn runs the engine starts it
  // right after that turn, as a typed Send used to.
  async function sendQueuedNow(itemId: string) {
    const runId = currentRunId.value
    if (!runId || sendingFollowUp.value) return
    const item = queueStore.take(runId, itemId)
    if (!item) return
    const result = await sendMessage(runId, () => outgoingOf(item))
    if (result === 'declined') queueStore.requeue(runId, item, 'budget')
    else if (result === 'failed') queueStore.requeue(runId, item, 'failed')
  }

  function removeQueuedMessage(itemId: string) {
    if (currentRunId.value) queueStore.remove(currentRunId.value, itemId)
  }

  // Inline edit of one chip's text (its attachments stay).
  const editingQueuedId = ref<string | null>(null)
  const queuedEditText = ref('')
  function startQueuedEdit(itemId: string) {
    const msg = queuedMessages.value.find((m) => m.id === itemId)
    if (!msg) return
    editingQueuedId.value = itemId
    queuedEditText.value = msg.text
  }
  function saveQueuedEdit() {
    const itemId = editingQueuedId.value
    editingQueuedId.value = null
    if (itemId && currentRunId.value) queueStore.edit(currentRunId.value, itemId, queuedEditText.value)
  }
  function cancelQueuedEdit() {
    editingQueuedId.value = null
  }
  watch(currentRunId, () => { editingQueuedId.value = null })

  // Permission / question prompts no longer live in this view at all: the global
  // PermissionWindowManager opens one standalone window per run that is asking (this
  // run, a popped-out session, or a background run) and owns the resolve logic.

  // A file menu anywhere (including another webview) asking for an @mention here.
  let mentionUnlisten: UnlistenFn | null = null
  async function setupMentionBridge() {
    try {
      mentionUnlisten = await listen<{ projectPath: string; path: string }>(
        FILE_MENTION_EVENT,
        (e) => {
          const payload = e.payload
          // Ignore files from a project this screen isn't showing.
          if (!payload?.path || (payload.projectPath && payload.projectPath !== project.value?.path)) return
          mentionFileInComposer(payload.path)
        },
      )
    } catch {
      /* running outside the Tauri shell */
    }
  }

  // Stop first asks the engine to end its turn and waits a few seconds for it
  // (AC-13); meanwhile the button reads "Stopping…", and pressing it again
  // kills the session at once (the backend sees the stop in progress).
  const stoppingRunId = ref<string | null>(null)
  const stopping = computed(() => stoppingRunId.value !== null && stoppingRunId.value === currentRunId.value)

  async function handleCancel() {
    if (!currentRunId.value) return
    const id = currentRunId.value
    stoppingRunId.value = id
    try {
      await runsStore.cancelRun(id)
      live.setStatus(id, 'cancelled')
      setLocalRunStatus(id, 'cancelled')
    } catch (e) {
      toast.error(String(e))
    } finally {
      if (stoppingRunId.value === id) stoppingRunId.value = null
    }
  }


  onUnmounted(() => {
    mentionUnlisten?.()
    mentionUnlisten = null
  })

  return {
    runSettingsOpen, runSettingsRoot, engineBadgeLabel, engineBadgeIcon, runSettingsTitle,
    toggleRunSettingsMenu, closeRunSettingsMenu, onRunSettingsPointerDown, modelOptions,
    onModelSelect, contextTokens, contextModel, composerModelLabel, contextRateLimit,
    creatingSession, handleNewSession, handoffTarget, handingOff, startingEngineSession,
    engineSwitchBusy, closeHandoffDialog, doHandoff, startNewEngineSession, followUpInput,
    sendingFollowUp, draftQuotes, removeDraftQuote, setDraftQuoteNote, selectedAccountId,
    accountChoices, accountModelLabel, accountSelectDisabled,
    handleAccountChange, pendingImages, pendingFiles, isDraggingFile, addDroppedPaths, removePendingFile, onPickFiles, onComposerPaste,
    removePendingImage, chatEligible, chatPlaceholder, chatSendLabel, composerVisible,
    primaryDisabled, handlePrimaryAction, sendSlashCommand, composerExpanded, autoResizeComposer,
    toggleComposerExpand, composerEl, mentionOpen, mentionIndex, slashOpen, slashIndex, slashItems,
    selectSlash, onComposerInput, ensureProjectFiles, fileMatcher, fileExists, pathChecker, fileListVersion, openFile, mentionItems,
    selectMention, insertCaptureRef, composerChecklist,
    applyChecklist, linkPicked, onComposerKeydown, handleSendFollowUp, setupMentionBridge,
    handleCancel, stopping, selectionSurface, queuedMessages, queueNotice, handleQueue, sendQueuedNow,
    removeQueuedMessage, editingQueuedId, queuedEditText, startQueuedEdit, saveQueuedEdit,
    cancelQueuedEdit, editingMessage, rewindingEdit, cancelEditResend,
    ...chips,
  }
}

export type RunComposer = ReturnType<typeof useRunComposer>
