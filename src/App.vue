<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from 'vue'
import { RouterView, useRoute, useRouter } from 'vue-router'
import { type UnlistenFn } from '@tauri-apps/api/event'
import { useProjectsStore } from '@/stores/projects'
import { useFocusStore } from '@/stores/focus'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { useUILayoutStore } from '@/stores/uiLayout'
import { useFeaturesStore } from '@/stores/features'
import { useI18n } from 'vue-i18n'
import { setLocale } from '@/i18n'
import PermissionNotifier from '@/components/PermissionNotifier.vue'
import InterruptedRunsBanner from '@/components/InterruptedRunsBanner.vue'
import PermissionWindowManager from '@/components/PermissionWindowManager.vue'
import CalendarReminder from '@/components/CalendarReminder.vue'
import UpdateNotifier from '@/components/UpdateNotifier.vue'
import WhatsNewToast from '@/components/WhatsNewToast.vue'
import CommandPalette from '@/components/CommandPalette.vue'
import { useProjectSwitcher } from '@/composables/useProjectSwitcher'
import RemoteDesktopAlert from '@/components/remote/RemoteDesktopAlert.vue'
import { ConfirmModal, PromptModal, ToastHost } from '@/components/ui'
import ImageCompareHost from '@/components/ImageCompareHost.vue'
import SelectionToolbar from '@/components/SelectionToolbar.vue'
import AppPopoutHost from '@/components/app/AppPopoutHost.vue'
import AppTitlebar from '@/components/app/AppTitlebar.vue'
import AgentStatusStrip from '@/components/app/AgentStatusStrip.vue'
import AppSidebar from '@/components/app/AppSidebar.vue'
import { openSlackWindow } from '@/lib/slackWindow'
import { useToast } from '@/composables/useToast'
import { useRouteCaptureContext } from '@/composables/useRouteCaptureContext'
import { useMainWindowEvents } from '@/composables/useMainWindowEvents'
import { useVaultOpenGuard } from '@/composables/useVaultOpenGuard'
import { useFindShortcut } from '@/composables/usePageFind'
import { normalizeColorTheme } from '@/lib/colorThemes'
import { applyThemeMode } from '@/lib/themeMode'
import { openItemCreateWindow } from '@/lib/itemWindow'
import { openItemListWindow } from '@/lib/itemListWindow'
import { openSecretNotes } from '@/lib/secretNotesWindow'
import {
  ABOUT_ACTION, IS_MAC, applyAppMenu, hasNativeMenu, isMenuActionBound, listenMenuActions, menuAccelerator,
  registerMenuAction, runMenuAction,
} from '@/lib/appMenu'
import {
  NEW_SESSION_ACTION, OPEN_WINDOWS_ACTION, PREVIOUS_PROJECT_ACTION, matchesAccelerator,
} from '@/lib/keymap'
import { openPath, openUrl } from '@tauri-apps/plugin-opener'
import { appDataDir, join } from '@tauri-apps/api/path'
import { isRemoteMode } from '@/lib/ipc/remote'
import { remoteFs } from '@/lib/ipc/native'
import { NAV_ROUTES } from '@/lib/navigation'
import { fallbackPath } from '@/lib/featureGate'
import {
  IS_POPOUT_WINDOW,
  IS_SECRET_NOTES_WINDOW,
  IS_SESSION_WINDOW,
  SESSION_WINDOW_PROJECT_ID,
  SESSION_WINDOW_RUN_ID,
} from '@/lib/windowMode'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { useRunsStore } from '@/stores/runs'
import { refreshTray, buildSessionItems } from '@/lib/tray'
import { openOverlayOnFocusedWindow } from '@/lib/openWindowsActions'
import { useWindowRunStatus } from '@/composables/useWindowRunStatus'
import WindowsOverlay from '@/components/windows/WindowsOverlay.vue'
import { useBackgroundProcessesStore } from '@/stores/backgroundProcesses'
import OrphanProcessesPrompt from '@/components/processes/OrphanProcessesPrompt.vue'
import QuitProcessesDialog from '@/components/processes/QuitProcessesDialog.vue'

// Pop-out windows (flags read once in lib/windowMode) render a bare,
// chrome-less host (no sidebar / nav / background work): AppPopoutHost.
// A session pop-out is different from the bare pop-outs above: it reuses the full
// RunView (routed to one run) with the app chrome hidden, so it still needs the
// stores + router — only the sidebar / nav / docks / app-wide notifiers are cut.
// Captured at module load (see windowMode) so it survives the router rewriting
// the URL out from under us on the first navigation.
const isSessionWindow = IS_SESSION_WINDOW
// All pop-out kinds only need the theme applied; skip the main app's data work.
const isPopoutWindow = IS_POPOUT_WINDOW

const route = useRoute()
const router = useRouter()
const { vaultOpenHere } = useVaultOpenGuard()
const projectsStore = useProjectsStore()
// Focus board for the sidebar badge (number of overdue todos).
const focusStore = useFocusStore()
const appSettings = useAppSettingsStore()
const tabsStore = useWorkspaceTabsStore()
const uiLayout = useUILayoutStore()
const live = useLiveRunsStore()
const runsStore = useRunsStore()
const bgProcs = useBackgroundProcessesStore()
const features = useFeaturesStore()
const { toast } = useToast()
const { t, locale } = useI18n()

// Session pop-out: drive the router to the run as early as possible (in setup,
// before first paint) so the window lands on RunView instead of flashing the
// default `/projects` redirect on the way there.
if (isSessionWindow && SESSION_WINDOW_PROJECT_ID && SESSION_WINDOW_RUN_ID) {
  router
    .replace({
      name: 'project-run-detail',
      params: { projectId: SESSION_WINDOW_PROJECT_ID, runId: SESSION_WINDOW_RUN_ID },
    })
    .catch(() => {})
}

// Session pop-out: keep the OS window title on the run's name so the menu-bar
// switcher (lib/tray.ts) can label this window, and refresh the tray whenever the
// title lands or is renamed.
if (isSessionWindow && SESSION_WINDOW_RUN_ID) {
  const runId = SESSION_WINDOW_RUN_ID
  watch(
    () =>
      runsStore.runMeta.get(runId)?.title ??
      runsStore.runs.find((r) => r.id === runId)?.title ??
      null,
    async (title) => {
      const name = (title && title.trim()) || t('tray.untitledSession')
      try {
        await getCurrentWebviewWindow().setTitle(name)
      } catch {
        /* outside the Tauri shell */
      }
      void refreshTray()
    },
    { immediate: true },
  )
}

// Main window owns the menu-bar badge (the running-session count) and the tray's
// running-session section. Refresh the tray whenever either changes — sessions
// starting / finishing / blocking on a permission, or being renamed — even for
// runs that were never popped out into their own window. Debounced: a burst of
// stream events shouldn't rebuild the native menu on every one.
if (!isPopoutWindow && !isSessionWindow) {
  useWindowRunStatus()
  let trayTimer: ReturnType<typeof setTimeout> | null = null
  watch(
    () => `${live.runningIds.length}|${JSON.stringify(buildSessionItems())}`,
    () => {
      if (trayTimer) clearTimeout(trayTimer)
      trayTimer = setTimeout(() => {
        trayTimer = null
        void refreshTray()
      }, 150)
    },
  )

  // A session whose `run:done` was missed would stay "running" forever and keep
  // the badge lit; poll the backend's live runs while any session claims to run.
  let reconcileTimer: ReturnType<typeof setInterval> | null = null
  watch(
    () => live.runningIds.length > 0,
    (anyRunning) => {
      if (anyRunning && !reconcileTimer) {
        reconcileTimer = setInterval(() => void live.reconcileRunning(), 10_000)
      } else if (!anyRunning && reconcileTimer) {
        clearInterval(reconcileTimer)
        reconcileTimer = null
      }
    },
    { immediate: true },
  )
}

// Tray rows and pop-outs asking the main window to open a run / a screen / a
// new session (attached in onMounted, main window only).
const { listenMainWindowEvents } = useMainWindowEvents()

// The page on screen whose feature was just switched off (Settings, another
// window, a remote desktop) moves away, as the router guard would on entry.
watch(
  () => features.tree,
  (tree) => {
    const to = features.loaded && !isPopoutWindow ? fallbackPath(route.path, NAV_ROUTES, tree) : null
    if (to) void router.replace(to)
  },
)

const isRunRoute = computed(
  () => route.name === 'project-run' || route.name === 'project-run-detail',
)

// Force a clean RunView remount only when the *project* changes (heavy stream
// state survives in the liveRuns store); switching runs within a project is
// handled in-place by RunView's own activeRunId watcher. Non-run routes keep a
// stable per-view key so their existing reuse behaviour is unchanged.
const routeKey = computed(() =>
  isRunRoute.value ? `run-${route.params.projectId}` : String(route.name ?? route.path),
)

// Views kept alive across navigation, so returning to one is instant: no
// teardown, no setup() re-run, no DOM rebuild, and scroll position + form state
// survive. Each still refreshes its data on re-entry via `useActivatedRefresh`.
//
// Matched by COMPONENT NAME, which is why every view listed here declares
// `defineOptions({ name })` rather than relying on the name Vue infers from the
// filename.
//
// RunView is deliberately absent: its heavy stream state already lives in the
// liveRuns store (so it survives navigation anyway) and `routeKey` exists
// precisely to force it to remount when the project changes. Do not add it.
//
// Only views with no timers, no event listeners and no cleanup hooks are listed.
// Anything with a lifecycle side effect needs its `onMounted`/`onUnmounted` work
// moved to `onActivated`/`onDeactivated` first — under KeepAlive those hooks no
// longer fire on navigation, so a timer would otherwise run forever in the
// background.
const CACHED_VIEWS = [
  'ServersView',
  'SessionSearchView',
]

// Pin every project the user opens as a workspace tab; remember the run being
// viewed so re-selecting the tab returns to it. Any project screen (settings
// too) makes it the most recently used one, for the switcher and ⌥⌘`.
watch(
  () => [route.name, route.params.projectId, route.params.runId] as const,
  ([name, projectId, runId]) => {
    // A session pop-out is locked to one run; it must not pin workspace tabs.
    if (isSessionWindow) return
    if (typeof projectId === 'string') tabsStore.touch(projectId)
    if ((name === 'project-run' || name === 'project-run-detail') && typeof projectId === 'string') {
      tabsStore.open(projectId, typeof runId === 'string' ? runId : null)
    }
  },
  { immediate: true },
)

// True when the event target is a text-entry surface where Backspace/navigation
// keys are legitimately used to edit text.
function isEditableTarget(el: EventTarget | null): boolean {
  const node = el as HTMLElement | null
  if (!node || typeof node.closest !== 'function') return false
  if (node.isContentEditable) return true
  return !!node.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]')
}

// Swallow navigation keys (mainly Backspace) when focus is NOT in a text field.
// Vietnamese IMEs (Telex/VNI) compose diacritics by injecting Backspace at the
// OS level; if the composer hasn't been focused yet, those Backspaces reach the
// webview and trigger history "back" navigation — the app appears to jump to a
// different screen while the user is just typing. Blocking them here is the
// backstop; RunView still focuses the composer on session open.
function onGlobalNavKeyGuard(e: KeyboardEvent) {
  if (e.key === 'Backspace' && !isEditableTarget(e.target)) {
    e.preventDefault()
  }
}

// Without the native menu (browser dev) its accelerators never fire, so the
// switcher's shortcuts are matched here instead — as CommandPalette does for ⌘K.
const SWITCHER_ACTIONS = [NEW_SESSION_ACTION, PREVIOUS_PROJECT_ACTION, 'file.openProject']
function onSwitcherShortcut(e: KeyboardEvent) {
  if (hasNativeMenu() || e.defaultPrevented || e.isComposing) return
  const id = SWITCHER_ACTIONS.find((a) => {
    const accel = menuAccelerator(a)
    return !!accel && matchesAccelerator(e, accel, IS_MAC)
  })
  if (!id) return
  e.preventDefault()
  runMenuAction(id)
}

// ⌘F opens the find bar of the area being read, in every window but the vault.
// Not in the keymap: a menu accelerator only reaches the main window, so ⌘F is
// one of the keys lib/keymap.ts refuses to give an action.
useFindShortcut()

// ── Native menu bar ─────────────────────────────────────────────────────────
// Every menu action that isn't screen-specific is bound here, at the app root,
// so it works from any route. New Session is bound here too, for the project on
// screen or the last one used; RunView binds it again on top while open, so
// there it keeps the composer's engine choice.
const switcher = useProjectSwitcher()
const routeCaptureContext = useRouteCaptureContext()

/** Open the titlebar switcher. */
function openSwitcher() {
  switcher.open.value = true
}

function bindGlobalMenuActions() {
  const unbinds = [
    registerMenuAction(NEW_SESSION_ACTION, () => switcher.newSessionAnywhere(), { fallback: true }),
    registerMenuAction('file.openProject', () => openSwitcher()),
    registerMenuAction(PREVIOUS_PROJECT_ACTION, () => switcher.goToPreviousProject()),
    // A NEW todo / note is ALWAYS captured in the standalone capture window, from
    // every entry point — menu, shortcut, run screen (existing ones are
    // read and edited in the Notes / Todos windows).
    registerMenuAction('file.newTodo', () =>
      openItemCreateWindow('todo', routeCaptureContext()),
    ),
    registerMenuAction('file.newNote', () =>
      openItemCreateWindow('note', routeCaptureContext()),
    ),
    registerMenuAction('window.todoList', () =>
      openItemListWindow('todo', { projectId: routeCaptureContext().projectId }),
    ),
    registerMenuAction('window.noteList', () =>
      openItemListWindow('note', { projectId: routeCaptureContext().projectId }),
    ),
    registerMenuAction('window.secretNotes', () =>
      openSecretNotes(router, { projectId: routeCaptureContext().projectId }),
    ),
    registerMenuAction('window.slackThreads', () =>
      openSlackWindow({ projectId: routeCaptureContext().projectId }),
    ),
    registerMenuAction(OPEN_WINDOWS_ACTION, () => void openOverlayOnFocusedWindow()),
    registerMenuAction(ABOUT_ACTION, () => void router.push('/settings/about').catch(() => {})),
    registerMenuAction('view.toggleSidebar', () => uiLayout.toggleSidebar()),
    // Windows/Linux only — macOS uses the OS full-screen item (see appMenu.ts).
    registerMenuAction('view.toggleFullscreen', async () => {
      const win = getCurrentWebviewWindow()
      await win.setFullscreen(!(await win.isFullscreen())).catch(() => {})
    }),
    registerMenuAction('help.docs', () => void openUrl(DOCS_URL).catch(() => {})),
    registerMenuAction('help.reportIssue', () => void openUrl(NEW_ISSUE_URL).catch(() => {})),
    registerMenuAction('help.openLogs', async () => {
      try {
        // Same folder lib.rs points the tracing file appender at. A remote
        // window shows the controlled machine's (its paths are that machine's).
        const dir = isRemoteMode() ? await remoteFs.logDir() : await join(await appDataDir(), 'logs')
        await openPath(dir)
      } catch (e) {
        toast.error(String(e))
      }
    }),
    registerMenuAction('help.reload', () => window.location.reload()),
    ...NAV_ROUTES.map((r) =>
      registerMenuAction(`go.${r.path}`, () => router.push(r.path)),
    ),
  ]
  return () => unbinds.forEach((off) => off())
}

const DOCS_URL = 'https://github.com/v9n8/Puppeteer#readme'
const NEW_ISSUE_URL = 'https://github.com/v9n8/Puppeteer/issues/new'

let unbindMenuActions: (() => void) | null = null
let unlistenMenu: UnlistenFn | null = null

// Labels carry both the language AND the current layout state ("Hide" vs "Show
// Sidebar"), New Session is greyed out unless a project screen is mounted, and
// the items carry the keymap's shortcuts, so the menu is rebuilt whenever any of
// those moves. Watching the i18n
// locale rather than the settings row avoids rebuilding with stale labels: the
// setting is what *drives* the switch, `locale` is what has actually applied.
watch(
  () =>
    [
      locale.value,
      uiLayout.sidebarMode,
      isMenuActionBound('file.newSession'),
      features.tree,
      appSettings.settings?.keymap_config,
    ] as const,
  () => {
    if (isPopoutWindow || isSessionWindow) return
    applyAppMenu({ sidebarMode: uiLayout.sidebarMode })
  },
)

// Rebuild the tray on language change so its static labels (Main Window / Quit)
// follow the app language, matching the app menu above; and on a feature switch,
// which adds or drops its quick actions.
watch([locale, () => features.tree], () => {
  if (isSessionWindow) return
  void refreshTray()
})

onBeforeUnmount(() => {
  unbindMenuActions?.()
  unbindMenuActions = null
  unlistenMenu?.()
  unlistenMenu = null
  window.removeEventListener('keydown', onGlobalNavKeyGuard, true)
  window.removeEventListener('keydown', onSwitcherShortcut)
})

onMounted(() => {
  window.addEventListener('keydown', onGlobalNavKeyGuard, true)
  window.addEventListener('keydown', onSwitcherShortcut)
})

const isDark = ref(false)

let stopThemeListener = () => {}

function applyTheme(theme: string) {
  stopThemeListener()
  stopThemeListener = applyThemeMode(
    theme,
    document.documentElement,
    window.matchMedia('(prefers-color-scheme: dark)'),
    (dark) => (isDark.value = dark),
  )
}

// Named color palette. Sets the `data-theme` attribute on <html> (see main.css);
// the default (indigo) theme uses no attribute so it falls back to :root/.dark.
function applyColorTheme(theme: string) {
  const t = normalizeColorTheme(theme)
  if (t !== 'default') document.documentElement.setAttribute('data-theme', t)
  else document.documentElement.removeAttribute('data-theme')
}

onMounted(async () => {
  // Every window kind, pop-outs included: each gates its own entries and must
  // follow a switch flipped elsewhere.
  void features.start()
  if (isPopoutWindow) {
    // Pop-out window: only theme matters; skip the main app's data fetches.
    try {
      await appSettings.refresh()
      applyTheme(appSettings.settings?.theme ?? 'system')
      applyColorTheme(appSettings.settings?.color_theme ?? 'default')
      setLocale(appSettings.settings?.language ?? 'en')
    } catch {
      applyTheme('system')
    }
    return
  }
  if (isSessionWindow) {
    // Chrome-less run window: apply the theme, then let RunView (already routed to
    // the run in setup) fetch its own projects / runs and open its stream. Skip
    // the main window's app-wide services — native menu, the
    // run:activated mirror and PermissionNotifier — so a popped-out session can't
    // double-fire OS notifications the main window already owns.
    try {
      await appSettings.refresh()
      applyTheme(appSettings.settings?.theme ?? 'system')
      applyColorTheme(appSettings.settings?.color_theme ?? 'default')
      setLocale(appSettings.settings?.language ?? 'en')
    } catch {
      applyTheme('system')
    }
    return
  }
  // The board behind the sidebar badge and the tray row refetches every 60 s:
  // only while Focus is on, stopped as soon as it is switched off.
  let focusPolling = false
  watch(
    () => features.loaded && features.isOn('productivity.focus'),
    (on) => {
      if (on === focusPolling) return
      focusPolling = on
      if (on) void focusStore.start()
      else focusStore.stop()
    },
    { immediate: true },
  )
  // Tracked background processes: counts for the sidebar row + tray, and the
  // first snapshot the startup orphan prompt waits on.
  void bgProcs.start()

  // Native menu bar: bind the actions first, then install the menu, so an
  // impatient click on a freshly drawn item can't land on nothing.
  unbindMenuActions = bindGlobalMenuActions()
  try {
    unlistenMenu = await listenMenuActions()
  } catch {
    // Ignore (e.g. running outside the Tauri shell during dev in a browser).
  }
  applyAppMenu({ sidebarMode: uiLayout.sidebarMode })

  // Populate the menu-bar window switcher (just the main window at first; session
  // pop-outs re-send as they open — see lib/tray.ts / lib/sessionWindow.ts).
  void refreshTray()

  await listenMainWindowEvents()

  // Load projects up front so app-wide UI (e.g. permission notifications) can
  // resolve project names without waiting for the Projects view to open.
  projectsStore.fetchProjects()
  projectsStore.fetchConflicts()
  projectsStore.fetchRuleConflicts()
  try {
    await appSettings.refresh()
    applyTheme(appSettings.settings?.theme ?? 'system')
    applyColorTheme(appSettings.settings?.color_theme ?? 'default')
    setLocale(appSettings.settings?.language ?? 'en')
  } catch {
    applyTheme('system')
  }
})
</script>

<template>
  <!-- Every pop-out kind and the session pop-out (a gated pop-out whose
       feature is off is always one of the pop-outs, so it lands here too). -->
  <AppPopoutHost v-if="isPopoutWindow || isSessionWindow" />

  <div v-else class="app-shell flex flex-col h-screen bg-background text-foreground overflow-hidden">
    <!-- Unified title bar: brand + open-project tabs + global actions on ONE 40px
         row spanning the whole window, above both columns.

         This is the app's only chrome row. The sidebar has no brand header of
         its own any more, so nothing competes with a view's header for the top
         line, and the run screen gains back ~52px of height for the conversation. -->
    <AppTitlebar />
    <AgentStatusStrip />

    <div class="flex flex-1 min-h-0">
    <!-- Sidebar. The title-bar toggle swaps it between the full column and a
         52px icon rail (all routes, persisted). Only focus mode hides it, and
         only while in the run workspace, so other routes like project settings
         keep their navigation. -->
    <AppSidebar :mode="uiLayout.sidebarMode" />

    <!-- Main content -->
    <main class="flex-1 min-w-0 flex flex-col overflow-hidden">
      <div class="flex-1 min-w-0 overflow-auto">
        <!-- `:key` stays on the inner <component>, NOT on <RouterView>: KeepAlive
             caches entries by their vnode key, and a key on the RouterView would
             tear the whole cache down on every navigation — the opposite of the
             point. `routeKey` keeps its existing contract either way (see above):
             run routes get `run-<projectId>` so RunView still remounts per
             project, other routes get a stable per-view key. -->
        <RouterView v-slot="{ Component }">
          <KeepAlive :include="CACHED_VIEWS" :max="8">
            <component :is="Component" :key="routeKey" />
          </KeepAlive>
        </RouterView>
      </div>
    </main>
    </div>

    <!-- Headless: fires native OS notifications for runs awaiting input while the
         app is backgrounded; the in-app signal is the History attention icon. -->
    <PermissionNotifier />

    <!-- Sessions cut off when the app last closed: resume / choose / dismiss.
         Only a click resumes one. A remote-desktop window drives another
         machine, whose own window shows them. -->
    <InterruptedRunsBanner v-if="!isRemoteMode()" />

    <!-- Headless: opens one standalone permission window per run that is asking
         (viewed, popped-out, or background) and owns the resolve logic. -->
    <PermissionWindowManager />

    <!-- Headless: fires native reminders for upcoming calendar events app-wide;
         clicking opens the event's detail drawer on the Calendar screen.
         Unmounted (timers cleared) while the Calendar or Google is off. -->
    <CalendarReminder v-if="features.isOn('productivity.calendar') && features.isOn('integrations.google')" />

    <!-- Offers a newer release of THIS app; a remote-desktop window drives
         another machine, which cannot be updated from here (install_update is
         local-only). -->
    <UpdateNotifier v-if="features.isOn('app.auto_update') && !isRemoteMode()" />

    <!-- Notes of THIS app's new version, once; a remote-desktop window runs
         another machine's version. -->
    <WhatsNewToast v-if="features.isOn('app.whats_new') && !isRemoteMode()" />

    <!-- ⌘K / Ctrl+K. Main window only: pop-outs (the Secret vault among them)
         and session windows have none. -->
    <CommandPalette v-if="features.isOn('app.command_palette')" />

    <!-- "Open windows" (⌘⇧E): under the title bar, so its button stays in sight. -->
    <WindowsOverlay :top-offset="40" />

    <!-- Startup prompt for processes left running by the previous app session -->
    <OrphanProcessesPrompt />

    <!-- Answers the backend's quit prompt when background processes are running -->
    <QuitProcessesDialog />

    <!-- App-wide confirm dialog host (see useConfirm) -->
    <ConfirmModal />

    <!-- App-wide text-input dialog host (see usePrompt) -->
    <PromptModal />

    <!-- App-wide toast host (see useToast) -->
    <ToastHost />

    <!-- Remote-desktop window: shown only while the link is down -->
    <RemoteDesktopAlert />

    <!-- App-wide side-by-side image comparison (banner + picker + compare view) -->
    <ImageCompareHost />
  </div>

  <!-- The window's one selection toolbar, for whichever screen declared a
       selection surface. Never in the Secret vault window, nor while the vault is
       open in this one: its text must not reach Translate or the clipboard shortcuts. -->
  <SelectionToolbar v-if="!IS_SECRET_NOTES_WINDOW && !vaultOpenHere" />
</template>
