// The composer and everything that sends from it: run settings (model, Claude
// account, permission), the per-session draft with attachments, quotes, slash
// commands, @-mentions, saved prompts, and start / follow-up / steer / cancel /
// engine handoff. Part of the RunView split (see useRunSession.ts).
import { ref, computed, onBeforeUnmount, onUnmounted, watch, nextTick } from 'vue'
import type { RunRecord, ProjectEntry } from '@/stores/runs'
import { invoke } from '@/lib/tauri'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { Cpu, Terminal } from 'lucide-vue-next'
import { type SavedPrompt, parseSavedPrompts } from '@/lib/savedPrompts'
import { useSelectionSurface } from '@/composables/useSelectionSurface'
import { transcriptMarkdownSources } from '@/lib/selectionSurface'
import { appendQuote, type QuoteSource } from '@/lib/quote'
import { appendQuoteToDraft, registerQuoteComposer } from '@/lib/quoteAppend'
import { linkTargetRun, type LinkTarget } from '@/lib/sessionLinks'
import {
  latestChecklist, mergeChecklistIntoDraft, parseChecklistBlock, type TodoItem,
} from '@/lib/todoChecklist'
import { mergeContextModel } from '@/lib/contextLimits'
import type { TurnSide } from '@/lib/turnActions'
import { matchProjectFile } from '@/lib/fileLinks'
import { openFileWindow } from '@/lib/fileWindow'
import { FILE_MENTION_EVENT } from '@/lib/fileEvents'
import type { ImageAttachment } from '@/lib/streamEvents'
import {
  MODEL_OPTIONS, resolvePermissionMode, withCurrentModel, combinedModelOptions, engineForModel,
} from '@/lib/engineOptions'
import type { RunSessionCore } from './useRunSessionCore'
import type { RunConductor } from './useRunConductor'
import type { RunLog } from './useRunLog'
import type { RunHeaderInfo } from './useRunHeaderInfo'

interface PendingImage extends ImageAttachment {
  id: string
  url: string // data URL for the thumbnail preview
}

interface PendingFile {
  id: string
  name: string
  path: string
}

export function useRunComposer(core: RunSessionCore, conductor: RunConductor, log: RunLog, header: RunHeaderInfo) {
  const {
    t, runsStore, live, conductorStore, draftsStore, claudeStore, appSettings, modelCatalog, confirm,
    toast, projectId, project, engineOverride, modelOverride, loadedRunEngine, syncLoadedRunEngine,
    effectiveEngine, permissionMode, currentRunId, currentRun, currentIsSession, session,
    liveEntries, liveOutputLines, liveHasStream, currentStatus, currentSessionId, runLabel,
  } = core
  const {
    conductorMaxWorkers,
  } = conductor
  const {
    outputEl, historyEl, viewingLogRunId, viewingLog, historyEntries, historyHasStream,
    historyContextTokens, historyModel, isViewingHistory, sourceText, stickToBottom,
    scrollOutputToBottom, scrollMemory, bookmarkState, toggleBookmark, snapshotTurns, rewindTarget,
    clearHistoryView, loadRunLog,
  } = log
  const {
    captureContext,
  } = header

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
    // The status bar's account · model chip toggles this same menu; closing here
    // would let its click reopen it at once.
    if (target.closest('[data-run-settings-trigger]')) return
    if (target.closest('[data-app-select-dropdown]')) return
    closeRunSettingsMenu()
  }
  // One unified picker: Claude + Codex models in a single grouped list, so the
  // chosen model implies the engine and no separate engine selector is needed.
  // Option tables live in @/lib/engineOptions (shared with the remote controller):
  // Claude's is curated + annotated with the last validation sweep; Codex's is
  // whatever `codex debug models` last reported.
  //
  // The model that an empty (Default) pick resolves to, read reactively from the
  // global settings so changing the default in Settings is reflected here at once.
  const globalDefaultModel = computed(() =>
    (effectiveEngine.value === 'codex'
      ? appSettings.settings?.codex_model
      : appSettings.settings?.claude_model) || '',
  )
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
    const def = globalDefaultModel.value
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
    const resolvedNext = nextEngine || appSettings.settings?.default_engine || 'claude'
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
  // or the resolved global default. Empty only when nothing is known yet.
  const composerModelLabel = computed(() =>
    session.value?.model
    || historyModel.value
    || modelOverride.value
    || currentRun.value?.model
    || globalDefaultModel.value
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

  // Preflight the run-blocking guardrail for `engine` BEFORE the UI echoes the
  // user's message or flips a run into "running". Returns the override flag to
  // pass to the backend (false = under budget, true = user chose to proceed
  // anyway), or `null` when the user declines — in which case the caller must
  // abort without touching the timeline.
  async function preflightBudget(engine: string): Promise<boolean | null> {
    let isOver = false
    try {
      const status = await invoke<{ is_over: boolean }>('get_run_budget', {
        engine,
        runId: currentRunId.value,
        projectId: projectId.value,
      })
      isOver = status.is_over
    } catch {
      return false // can't determine the verdict → don't block the user
    }
    if (!isOver) return false
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
      await runsStore.startRun(
        id,
        engine,
        permissionMode.value || undefined,
        undefined,
        modelOverride.value || undefined,
        undefined,
        override,
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
        permissionModeOverride: resolvePermissionMode(
          permissionMode.value,
          appSettings.settings?.default_permission_mode,
        ),
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
      const { run, context_path } = await runsStore.createHandoffRun(sourceId, targetEngine, transcript)
      await runsStore.fetchRuns(projectId.value) // surface the new run in the sidebar
      syncLoadedRunEngine(targetEngine)
      // Carry the model the user picked when they triggered the switch.
      if (pendingEngineModel.value !== null) modelOverride.value = pendingEngineModel.value
      const seed = t('run.handoffSeed', { engine: sourceEngine, path: context_path })
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
  const sendingFollowUp = ref(false)
  const savingClaudeAccount = ref(false)

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
  const claudeAccountOptions = computed(() => {
    const opts = claudeStore.accounts.map((acc) => ({
      value: acc.id,
      label: `${acc.label}${acc.email ? ` · ${acc.email}` : ''}`,
    }))
    const currentId = currentRun.value?.claude_account_id
    if (currentId && !opts.some((opt) => opt.value === currentId)) {
      opts.unshift({ value: currentId, label: t('run.missingClaudeAccount') })
    }
    // Always offer the Default tier so a per-session pick can be cleared back to
    // "follow project/global".
    opts.unshift({ value: '', label: defaultClaudeAccountLabel.value })
    return opts
  })
  const showClaudeAccountSelect = computed(() =>
    !!currentRun.value
    && effectiveEngine.value === 'claude'
    && (claudeStore.accounts.length > 0 || !!currentRun.value.claude_account_id),
  )
  // The Claude account this session runs on (its own pick, else the
  // project/global default), for the status bar's account · model chip.
  // '' when the run-settings menu offers no account choice.
  const activeClaudeAccountLabel = computed(() => {
    if (!showClaudeAccountSelect.value) return ''
    const id = selectedClaudeAccountId.value || effectiveProjectClaudeAccountId.value
    return claudeStore.accounts.find((acc) => acc.id === id)?.label ?? ''
  })
  const claudeAccountSelectDisabled = computed(() =>
    currentStatus.value === 'running' || sendingFollowUp.value || savingClaudeAccount.value,
  )
  const claudeAccountSelectTitle = computed(() =>
    currentStatus.value === 'running'
      ? t('run.claudeAccountRunningTitle')
      : t('run.claudeAccountTitle'),
  )

  async function handleClaudeAccountChange(value: string) {
    const run = currentRun.value
    if (!run || currentStatus.value === 'running' || savingClaudeAccount.value) return
    const next = value.trim() || null
    if ((run.claude_account_id ?? null) === next) return
    savingClaudeAccount.value = true
    try {
      await runsStore.setRunClaudeAccount(run.id, next)
      toast.success(t('run.claudeAccountUpdated'))
    } catch (e) {
      toast.error(t('run.claudeAccountUpdateFailed', { error: String(e) }))
    } finally {
      savingClaudeAccount.value = false
    }
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

  function restoreDraftFor(runId: string | null) {
    restoringDraft = true
    pendingImages.value = []
    pendingFiles.value = []
    followUpInput.value = ''
    if (runId) {
      const draft = draftsStore.get(runId)
      followUpInput.value = draft.text
      for (const img of draft.images) pushPendingImage(img.media_type, img.data)
      for (const f of draft.files) addPendingFile(f.path)
    }
    nextTick(() => { restoringDraft = false })
  }

  // Load the focused session's draft whenever it changes (initial open, session
  // switch, new session). The persist watcher below then tracks edits for it.
  watch(currentRunId, (id) => restoreDraftFor(id), { immediate: true })

  watch(
    [followUpInput, pendingImages, pendingFiles],
    () => {
      if (restoringDraft || !currentRunId.value) return
      draftsStore.set(currentRunId.value, {
        text: followUpInput.value,
        images: pendingImages.value.map(({ media_type, data }) => ({ media_type, data })),
        files: pendingFiles.value.map(({ name, path }) => ({ name, path })),
      })
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
        append: (text: string) => {
          followUpInput.value = appendQuote(followUpInput.value, text)
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
  function takeQuote(text: string, source: QuoteSource) {
    if (!sendingFollowUp.value) {
      followUpInput.value = appendQuote(followUpInput.value, text, source)
      return
    }
    // The send clears the composer when it returns: add the quote after that.
    const runId = currentRunId.value
    if (!runId) return
    const stop = watch(sendingFollowUp, (busy) => {
      if (busy) return
      stop()
      if (currentRunId.value === runId) followUpInput.value = appendQuote(followUpInput.value, text, source)
      else appendQuoteToDraft(runId, text, source)
    })
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
    if (currentStatus.value === 'running') return t('run.sendLabelSend')
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
    if (sendingFollowUp.value) return true
    // A fresh session launches as a conductor, which needs a goal to start.
    if (currentStatus.value === 'fetched' && currentRun.value?.run_type === 'session') {
      return !followUpInput.value.trim() && !pendingImages.value.length && !pendingFiles.value.length
    }
    if (currentStatus.value === 'fetched' || !chatEligible.value) return false
    return !followUpInput.value.trim() && !pendingImages.value.length && !pendingFiles.value.length
  })

  // One entry point for the footer's primary button: cancel while running is a
  // separate button, so here we either send a typed message/resume, or kick off
  // the default-prompt run when nothing was typed.
  function handlePrimaryAction() {
    if (currentStatus.value === 'running') { handleSendFollowUp(); return }
    const hasInput = followUpInput.value.trim().length > 0 || pendingImages.value.length > 0 || pendingFiles.value.length > 0
    if (!hasInput && (currentStatus.value === 'fetched' || !chatEligible.value)) {
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
    try {
      projectFiles.value = await runsStore.listProjectFiles(path)
      projectFilesPath.value = path
    } catch {
      projectFiles.value = []
    }
  }

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
    return matchProjectFile(raw, projectFileSet.value)
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

  // ── Saved prompt library (edited in Settings → Prompt Templates) ──────────
  const savedPrompts = computed(() => parseSavedPrompts(appSettings.settings?.saved_prompts))

  // Insert a saved prompt at the caret, appending when the composer isn't
  // focused — same contract as `mentionFileInComposer`. Deliberately does NOT
  // send: the user reviews/edits first.
  function insertSavedPrompt(p: SavedPrompt) {
    const el = composerEl.value
    const cur = followUpInput.value
    const focused = el && document.activeElement === el
    const caret = focused ? (el!.selectionStart ?? cur.length) : cur.length
    const before = cur.slice(0, caret)
    const after = cur.slice(caret)
    // Keep an existing draft readable by dropping the prompt onto its own line.
    const sep = before.length && !/\n$/.test(before) ? '\n' : ''
    const insert = `${sep}${p.body}`
    followUpInput.value = before + insert + after
    nextTick(() => {
      const pos = (before + insert).length
      if (el) { el.focus(); el.setSelectionRange(pos, pos) }
    })
  }

  // A reference from CapturePicker (Slack / web / note / todo) goes in like a saved prompt.
  function insertCaptureRef(text: string) {
    insertSavedPrompt({ id: '', title: '', body: text })
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
    // Enter (no Shift) sends; Shift+Enter inserts a newline.
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSendFollowUp()
    }
  }

  // `steer`: add the message to the turn in flight (sessions.steer) instead of
  // sending a follow-up. A turn that ended meanwhile gets a plain follow-up.
  async function handleSendFollowUp(steer = false) {
    mentionOpen.value = false
    const text = followUpInput.value.trim()
    const hasImages = pendingImages.value.length > 0
    const hasFiles = pendingFiles.value.length > 0
    if ((!text && !hasImages && !hasFiles) || !currentRunId.value || sendingFollowUp.value) return
    const id = currentRunId.value
    const isFetched = currentStatus.value === 'fetched'
    const wasRunning = currentStatus.value === 'running'
    const runEngine =
      (!isFetched && (loadedRunEngine.value || currentRun.value?.engine)) || effectiveEngine.value
    sendingFollowUp.value = true
    try {
      // Always confirm the budget BEFORE echoing anything into the timeline. On
      // decline nothing is pushed and no "thinking" indicator appears.
      const override = await preflightBudget(runEngine)
      if (override === null) return

      const images = takePendingImages()
      const prompt = composePrompt(text, takePendingFilePaths())

      if (isFetched) {
        // First message of a fetched run (fresh session or issue/PR): it becomes
        // the conductor goal.
        await launchConductor(id, prompt, images, override)
        followUpInput.value = ''
        return
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
      stickToBottom.value = true // re-pin: the user just sent a message
      scrollMemory.forget(id)
      scrollOutputToBottom()

      if (!wasRunning) {
        live.setStatus(id, 'running')
        setLocalRunStatus(id, 'running')
        await live.startListening(id, projectId.value)
        await runsStore.resumeRun(id, permissionMode.value || undefined, modelOverride.value || undefined, override)
      }

      if (steer && wasRunning) {
        // No turn of its own, so no snapshot and no "Rewind to here".
        await runsStore.steerRun(id, prompt, images, override)
      } else {
        const turn = await runsStore.sendUserMessage(id, prompt, images, override)
        // The snapshot turn it was logged under: "Rewind to here" finds it by this.
        if (turn !== null && sent.kind === 'user') sent.turn = turn
      }
      followUpInput.value = ''
    } catch (e) {
      live.appendOutput(id, t('run.sendFailed', { error: String(e) }))
    } finally {
      sendingFollowUp.value = false
    }
  }

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

  async function handleCancel() {
    if (!currentRunId.value) return
    const id = currentRunId.value
    try {
      await runsStore.cancelRun(id)
      live.setStatus(id, 'cancelled')
      setLocalRunStatus(id, 'cancelled')
    } catch (e) {
      toast.error(String(e))
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
    sendingFollowUp, selectedClaudeAccountId, claudeAccountOptions, showClaudeAccountSelect,
    activeClaudeAccountLabel, claudeAccountSelectDisabled, claudeAccountSelectTitle,
    handleClaudeAccountChange, pendingImages, pendingFiles, isDraggingFile, addDroppedPaths, removePendingFile, onPickFiles, onComposerPaste,
    removePendingImage, chatEligible, chatPlaceholder, chatSendLabel, composerVisible,
    primaryDisabled, handlePrimaryAction, sendSlashCommand, composerExpanded, autoResizeComposer,
    toggleComposerExpand, composerEl, mentionOpen, mentionIndex, slashOpen, slashIndex, slashItems,
    selectSlash, onComposerInput, ensureProjectFiles, fileMatcher, openFile, mentionItems,
    selectMention, savedPrompts, insertSavedPrompt, insertCaptureRef, composerChecklist,
    applyChecklist, linkPicked, onComposerKeydown, handleSendFollowUp, setupMentionBridge,
    handleCancel, selectionSurface,
  }
}

export type RunComposer = ReturnType<typeof useRunComposer>
