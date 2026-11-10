// Shared state and actions of the titlebar project switcher (UX-09). The popover
// lives in the titlebar (ProjectSwitcher.vue), but its shortcuts — New Session,
// Previous Project, Open Project… — are bound app-wide in App.vue, so they work
// on every screen, focus mode included.
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { useProjectsStore } from '@/stores/projects'
import { useRunsStore } from '@/stores/runs'
import { useToast } from '@/composables/useToast'
import { runMenuAction } from '@/lib/appMenu'
import { NEW_SESSION_ACTION } from '@/lib/keymap'
import { previousProject } from '@/lib/projectSwitcher'

// Module-level: one popover per window, opened from the menu as well as its button.
const open = ref(false)
// One session at a time: a held ⌘T must not create a pile of them.
let creating = false

export function useProjectSwitcher() {
  const route = useRoute()
  const router = useRouter()
  const { t } = useI18n()
  const { toast } = useToast()
  const tabs = useWorkspaceTabsStore()
  const projects = useProjectsStore()
  const runs = useRunsStore()

  /** The project on screen: its sessions or its settings. */
  const currentProjectId = computed(() =>
    typeof route.params.projectId === 'string' ? route.params.projectId : null,
  )
  const onRunRoute = computed(() => route.name === 'project-run' || route.name === 'project-run-detail')

  /** Pin the project as a tab and go to the session last open there (else its newest). */
  function openProject(projectId: string) {
    tabs.open(projectId)
    if (onRunRoute.value && currentProjectId.value === projectId) return
    const lastRunId = tabs.tabs.find((tab) => tab.projectId === projectId)?.lastRunId
    const to = lastRunId
      ? { name: 'project-run-detail', params: { projectId, runId: lastRunId } }
      : { name: 'project-run', params: { projectId } }
    router.push(to).catch(() => {})
  }

  /** Create a session in `projectId` and open it (RunView mounts on it and loads it). */
  async function createSessionIn(projectId: string) {
    if (creating) return
    creating = true
    try {
      const run = await runs.createSessionRun(projectId)
      tabs.open(projectId, run.id)
      await router.push({ name: 'project-run-detail', params: { projectId, runId: run.id } })
    } catch (e) {
      toast.error(t('run.createSessionFailed', { error: String(e) }))
    } finally {
      creating = false
    }
  }

  /**
   * "New session" of a switcher row. On that project's own screen, the screen
   * starts it (New Session there keeps the composer's engine choice).
   */
  function newSession(projectId: string) {
    if (onRunRoute.value && currentProjectId.value === projectId) runMenuAction(NEW_SESSION_ACTION)
    else void createSessionIn(projectId)
  }

  /**
   * The app-wide New Session (⌘T) off the project screen: in the project on
   * screen, else the last one used; with neither, the switcher opens to pick one.
   */
  function newSessionAnywhere() {
    const target = currentProjectId.value ?? previousProject(tabs.recent, null, projects.projects)
    if (target) void createSessionIn(target)
    else open.value = true
  }

  /** ⌘⌥`: back to the project used before this one. */
  function goToPreviousProject() {
    const target = previousProject(tabs.recent, currentProjectId.value, projects.projects)
    if (target) openProject(target)
  }

  return { open, currentProjectId, openProject, newSession, newSessionAnywhere, goToPreviousProject }
}
