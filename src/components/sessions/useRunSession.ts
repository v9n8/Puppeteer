// RunView's state, shared with the components it is split into (UX-03).
//
// RunView creates it once (createRunSession) and each part under
// components/sessions injects it (useRunSession). Per-view rather than module
// state, so every watcher and listener stops when the view unmounts. The parts
// are built in the order their watchers used to be declared in RunView, so the
// watchers still fire in the same order.
import {
  onMounted, onBeforeUnmount, onUnmounted, nextTick, provide, inject, type InjectionKey,
} from 'vue'
import { registerMenuAction } from '@/lib/appMenu'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { remotePathsForDrop } from '@/lib/remoteUpload'
import { hitFromRouteQuery } from '@/lib/sessionSearch'
import { EVT_TRASH_CHANGED } from '@/lib/trash'
import { nativeDropPoint, treeDropTargetAt } from '@/lib/fileTreeDnd'
import { useRunSessionCore, type RunSessionCore } from './useRunSessionCore'
import { useRunConductor, type RunConductor } from './useRunConductor'
import { useRunLog, type RunLog, type RunLogLate } from './useRunLog'
import { useRunHeaderInfo, type RunHeaderInfo } from './useRunHeaderInfo'
import { useRunHistoryList, type RunHistoryList } from './useRunHistoryList'
import { useRunComposer, type RunComposer } from './useRunComposer'

export type RunSession = RunSessionCore & RunConductor & RunLog & RunHeaderInfo & RunHistoryList & RunComposer

const RUN_SESSION: InjectionKey<RunSession> = Symbol('runSession')

export function createRunSession(): RunSession {
  const core = useRunSessionCore()
  const conductor = useRunConductor(core)
  const late = {} as RunLogLate
  const log = useRunLog(core, late)
  const header = useRunHeaderInfo(core)
  const historyList = useRunHistoryList(core, conductor, log)
  const composer = useRunComposer(core, conductor, log, header)
  late.closeRunSettingsMenu = composer.closeRunSettingsMenu
  late.dismissSelection = composer.selectionSurface.dismiss
  const session: RunSession = { ...core, ...conductor, ...log, ...header, ...historyList, ...composer }
  useRunViewLifecycle(session)
  provide(RUN_SESSION, session)
  return session
}

export function useRunSession(): RunSession {
  const session = inject(RUN_SESSION)
  if (!session) throw new Error('useRunSession() called outside RunView')
  return session
}

// Mount / unmount work that spans the parts: the first load, app-wide menu and
// window listeners, file drops and the transcript watcher.

function useRunViewLifecycle(s: RunSession) {
  const {
    route, projectStore, runsStore, live, conductorStore, ghStore, glStore, awsStore, azureStore,
    claudeStore, appSettings, commandPalette, modelCatalog, toast, projectId, activeRunId,
    currentRunId, loadMarkdown, scrollMemory, onTurnNavKey, onWindowPointerUp, openRouteSearchHit,
    loadRunLog, loadProjectServers, repos, onRunSettingsPointerDown, handleNewSession,
    isDraggingFile, addDroppedPaths, composerVisible, autoResizeComposer, ensureProjectFiles,
    insertSavedPrompt, setupMentionBridge,
  } = s

  onMounted(async () => {
    loadMarkdown()
    nextTick(autoResizeComposer)
    document.addEventListener('mousedown', onRunSettingsPointerDown)
    // File → New Session. Only this screen can serve it: starting a session
    // needs the project the user is currently in.
    unbindNewSessionMenu = registerMenuAction('file.newSession', () => {
      void handleNewSession()
    })
    // A saved prompt picked in the command palette lands here, like one from the picker.
    unbindPaletteComposer = commandPalette.bindComposer((body) => insertSavedPrompt({ id: '', title: '', body }))
    // Hydrate the persisted model caches so the composer's picker shows the last
    // refreshed lists (no discovery here — refresh is manual in Settings).
    modelCatalog.ensureLoaded().catch(() => {})
    // Needed for the composer's saved-prompt dropdown (and the engine default).
    appSettings.ensureLoaded().catch(() => {})
    if (projectStore.projects.length === 0) {
      await projectStore.fetchProjects()
    }
    // GĐ6 (AC3): ensure account metadata is available for the header badge.
    if (ghStore.accounts.length === 0) ghStore.fetch().catch(() => {})
    if (glStore.accounts.length === 0) glStore.fetch().catch(() => {})
    if (claudeStore.accounts.length === 0) claudeStore.fetch().catch(() => {})
    // Header chips: which VPS servers + AWS account this project is wired to.
    if (awsStore.accounts.length === 0) awsStore.fetch().catch(() => {})
    if (azureStore.accounts.length === 0) azureStore.fetch().catch(() => {})
    loadProjectServers()
    repos.value = await projectStore.listRepos(projectId.value)
    // Opening the view is the one point where History takes the fresh activity
    // order; later background refetches keep the rows where they are.
    await runsStore.fetchRuns(projectId.value, { keepOrder: false })
    // Preload the project file list so inline-code mentions resolve to links.
    ensureProjectFiles().catch(() => {})
    if (activeRunId.value) {
      const searchHit = hitFromRouteQuery(route.query)
      // Open at the hit, not where the reader last left this run.
      if (searchHit) scrollMemory.forget(activeRunId.value)
      await loadRunLog(activeRunId.value)
      if (searchHit) await openRouteSearchHit(searchHit)
    } else {
      // Bare project route (no runId): reopen the most recent session so the user
      // lands back on their last conversation instead of an empty screen. Runs are
      // sorted pinned-first then last-activity desc, so the first one is the one
      // the user worked on most recently.
      // With no runs at all, the "chưa có session nào" empty state is shown.
      const latest = runsStore.runs[0]
      if (latest) await loadRunLog(latest.id)
    }
    window.addEventListener('focus', onAppFocus)
    window.addEventListener('pointerup', onWindowPointerUp)
    window.addEventListener('keydown', onTurnNavKey)
    setupMentionBridge()

    // Drag-and-drop files into the composer. Tauri intercepts OS file drops at the
    // native layer (dragDropEnabled defaults on), so the webview's HTML5 `drop`
    // carries no paths — we must use the native event, which gives absolute paths.
    // Drops landing on the file explorer belong to it (copied into the folder
    // under the cursor), so the composer ignores those positions.
    dragDropUnlisten = await getCurrentWebview().onDragDropEvent((event) => {
      if (!composerVisible.value) return
      const p = event.payload
      const overTree = p.type !== 'leave' && (() => {
        const { x, y } = nativeDropPoint(p.position)
        return treeDropTargetAt(x, y) !== null
      })()
      if (p.type === 'enter' || p.type === 'over') {
        isDraggingFile.value = !overTree
      } else if (p.type === 'leave') {
        isDraggingFile.value = false
      } else if (p.type === 'drop') {
        isDraggingFile.value = false
        // In a remote-desktop window the dropped files are copied over first.
        if (!overTree) {
          void remotePathsForDrop(p.paths)
            .then(addDroppedPaths)
            .catch((e) => toast.error(String(e)))
        }
      }
    })

    // Mirror in any sessions created/continued outside Puppeteer while it was closed
    // (both engines), then surface them in the sidebar.
    Promise.all([
      runsStore.reconcileClaudeSessions(projectId.value).catch(() => 0),
      runsStore.reconcileCodexSessions(projectId.value).catch(() => 0),
    ]).then(([a, b]) => { if (a + b > 0) runsStore.fetchRuns(projectId.value) })

    // Live mirror: the backend watches the shared Claude transcript store and
    // emits this when a session changes (new turns from the CLI / VS Code, or a
    // brand-new external session). Refetch the list and reload the viewed run.
    sessionsChangedUnlisten = await listen<{ project_id: string; run_id?: string }>(
      'sessions:changed',
      (event) => {
        if (event.payload?.project_id !== projectId.value) return
        // Always refresh the sidebar list. Only reload the on-screen log when the
        // run that changed is the one being viewed — other sessions in the project
        // changing shouldn't force a reload of the open conversation.
        runsStore.fetchRuns(projectId.value)
        if (event.payload.run_id && event.payload.run_id === currentRunId.value) {
          refreshViewedRunOnFocus(true)
        }
      },
    )
    // Sessions moved to / out of the Trash elsewhere (another window, the Trash
    // screen, a remote desktop): History follows.
    trashChangedUnlisten = await listen(EVT_TRASH_CHANGED, () => {
      runsStore.fetchRuns(projectId.value)
    })
  })

  // When the window regains focus, the run on screen may have been continued
  // outside Puppeteer (claude CLI / VS Code extension write to the same session
  // transcript). Re-read its on-disk log so those external turns show up.
  // Skip live sessions — those stream their own updates in real time.
  let sessionsChangedUnlisten: UnlistenFn | null = null
  let trashChangedUnlisten: UnlistenFn | null = null
  let dragDropUnlisten: UnlistenFn | null = null

  // Unbound on unmount so the menu item stops firing into a dead screen.
  let unbindNewSessionMenu: (() => void) | null = null
  let unbindPaletteComposer: (() => void) | null = null

  // `force` = the on-disk transcript actually changed (the sessions:changed file
  // watcher fired), so we must re-read even a run we hold live in memory. A plain
  // window refocus passes force=false: it must NOT blank + re-render the viewed
  // run (that was the visible "flash" on every refocus). loadRunLog then keeps a
  // live session on screen as-is, and for a history view re-reads but no-ops when
  // the content is byte-identical.
  async function refreshViewedRunOnFocus(force = false) {
    const id = currentRunId.value
    if (!id) return
    const ls = live.get(id)
    if (ls && ls.status === 'running') return
    await loadRunLog(id, { preferDisk: true, force })
  }

  // Wrapper for the window 'focus' event: the DOM passes an Event as the first
  // arg, which must not be read as `force` (it's truthy).
  function onAppFocus() {
    void refreshViewedRunOnFocus(false)
    // Safety-net for the event-driven conductor panel: catch any `conductor:changed`
    // event that may have been missed while the window was in the background.
    if (conductorStore.selectedId) void conductorStore.refreshDetail()
  }

  // Leaving the screen (e.g. switching project tabs remounts RunView): save the
  // reading position while the output is still in the DOM.
  onBeforeUnmount(() => scrollMemory.dispose())

  onUnmounted(() => {
    // Run event listeners live in the liveRuns store, so they intentionally
    // survive this component unmounting — that's what lets runs keep streaming
    // while the user is on another screen.
    unbindNewSessionMenu?.()
    unbindNewSessionMenu = null
    unbindPaletteComposer?.()
    unbindPaletteComposer = null
    conductorStore.stopPolling()
    window.removeEventListener('focus', onAppFocus)
    document.removeEventListener('mousedown', onRunSettingsPointerDown)
    window.removeEventListener('pointerup', onWindowPointerUp)
    window.removeEventListener('keydown', onTurnNavKey)
    sessionsChangedUnlisten?.()
    sessionsChangedUnlisten = null
    trashChangedUnlisten?.()
    trashChangedUnlisten = null
    dragDropUnlisten?.()
    dragDropUnlisten = null
  })
}
