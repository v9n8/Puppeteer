// Everything the command palette can run, as items of lib/commandPalette.
// Shared by the palette and Settings → Command palette (which only needs the
// labels of pinned / hidden ids). Menu-backed items run through the app menu's
// own handlers (lib/appMenu `runMenuAction`), so they behave exactly like the
// menu item and its feature gate.
import { computed, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import {
  BookMarked, FolderOpen, ListTodo, LockKeyhole, MessageSquare, Plus, Slack, StickyNote, Trash2,
} from 'lucide-vue-next'
import { NAV_ROUTES } from '@/lib/navigation'
import { MENU_ACTION_FEATURES } from '@/lib/featureGate'
import { isMenuActionBound, runMenuAction } from '@/lib/appMenu'
import { SETTINGS_SECTIONS, groupLabelKey } from '@/lib/settingsSections'
import { parseSavedPrompts, promptLabel } from '@/lib/savedPrompts'
import { focusSessionWindow } from '@/lib/sessionWindow'
import type { PaletteItem } from '@/lib/commandPalette'
import { useProjectsStore } from '@/stores/projects'
import { runActivityAt, useRunsStore } from '@/stores/runs'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useCommandPaletteStore } from '@/stores/commandPalette'
import { useToast } from '@/composables/useToast'

export interface PaletteCommand extends PaletteItem {
  icon: Component
  run: () => unknown
}

// Windows of the native Window menu, by action id.
const WINDOW_ACTIONS: readonly { id: string; labelKey: string; icon: Component }[] = [
  { id: 'window.todoList', labelKey: 'menu.todoList', icon: ListTodo },
  { id: 'window.noteList', labelKey: 'menu.noteList', icon: StickyNote },
  { id: 'window.secretNotes', labelKey: 'menu.secretNotes', icon: LockKeyhole },
  { id: 'window.slackThreads', labelKey: 'menu.slackThreads', icon: Slack },
]

// Sessions loaded this app session (every project opened so far), newest first.
const MAX_SESSIONS = 30

export function usePaletteItems() {
  const { t } = useI18n()
  const router = useRouter()
  const projects = useProjectsStore()
  const runs = useRunsStore()
  const appSettings = useAppSettingsStore()
  const palette = useCommandPaletteStore()
  const { toast } = useToast()

  async function openSession(runId: string, projectId: string) {
    if (await focusSessionWindow(runId)) return
    router.push({ name: 'project-run-detail', params: { projectId, runId } }).catch(() => {})
  }

  async function pickPrompt(body: string) {
    if (palette.insertIntoComposer(body)) return
    // No session on screen: hand it over through the clipboard instead.
    try {
      await navigator.clipboard.writeText(body)
      toast.success(t('palette.promptCopied'))
    } catch (e) {
      toast.error(String(e))
    }
  }

  const items = computed<PaletteCommand[]>(() => {
    const projectName = new Map(projects.projects.map((p) => [p.id, p.name]))

    const nav: PaletteCommand[] = [
      ...NAV_ROUTES.map((r) => ({
        id: `go.${r.path}`,
        group: 'nav' as const,
        label: t(r.labelKey),
        icon: r.icon,
        feature: r.feature,
        run: () => runMenuAction(`go.${r.path}`),
      })),
      ...WINDOW_ACTIONS.map((w) => ({
        id: w.id,
        group: 'nav' as const,
        label: t(w.labelKey),
        icon: w.icon,
        feature: MENU_ACTION_FEATURES[w.id],
        run: () => runMenuAction(w.id),
      })),
      // The Trash sits in the sidebar footer, not in NAV_ROUTES (nor the Go menu).
      { id: 'go./trash', group: 'nav', label: t('nav.trash'), icon: Trash2, run: () => router.push('/trash').catch(() => {}) },
    ]

    // Bound app-wide once the main window is up (the current or last-used project).
    const newSession: PaletteCommand[] = isMenuActionBound('file.newSession')
      ? [{ id: 'file.newSession', group: 'session', label: t('menu.newSession'), icon: Plus, run: () => runMenuAction('file.newSession') }]
      : []
    const sessions: PaletteCommand[] = [...runs.runMeta.values()]
      .filter((r) => projectName.has(r.project_id))
      .sort((a, b) => runActivityAt(b).localeCompare(runActivityAt(a)))
      .slice(0, MAX_SESSIONS)
      .map((r) => ({
        id: `session:${r.id}`,
        group: 'session',
        label: r.title?.trim() || t('palette.untitledSession'),
        detail: projectName.get(r.project_id),
        icon: MessageSquare,
        run: () => openSession(r.id, r.project_id),
      }))

    // Hidden projects stay out (UX-08); their sessions above still show.
    const projectItems: PaletteCommand[] = projects.projects.filter((p) => !p.hidden).map((p) => ({
      id: `project:${p.id}`,
      group: 'project',
      label: p.name,
      detail: p.path,
      icon: FolderOpen,
      run: () => router.push(`/projects/${p.id}`).catch(() => {}),
    }))

    const prompts: PaletteCommand[] = parseSavedPrompts(appSettings.settings?.saved_prompts).map((p) => ({
      id: `prompt:${p.id}`,
      group: 'prompt',
      label: promptLabel(p) || t('palette.untitledPrompt'),
      detail: p.title.trim() ? p.body.trim().split('\n')[0] : undefined,
      icon: BookMarked,
      run: () => pickPrompt(p.body),
    }))

    const settings: PaletteCommand[] = SETTINGS_SECTIONS.map((s) => ({
      id: `settings.${s.id}`,
      group: 'settings',
      label: t(s.labelKey),
      detail: `${t('nav.settings')} · ${t(groupLabelKey(s.group))}`,
      icon: s.icon,
      feature: 'feature' in s ? s.feature : undefined,
      run: () => router.push({ path: '/settings', query: { section: s.id } }).catch(() => {}),
    }))

    return [...nav, ...newSession, ...sessions, ...projectItems, ...prompts, ...settings]
  })

  return { items }
}
