// The right-click menu of a board card: the same items and rules as a History row
// (RunActionItems), with the handlers it needs outside the Session screen. The
// History row's own handlers live in sessions/useRunHistoryList (they lean on the
// open Session), so this is their small twin; both call the same runs-store commands.
import { reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener'
import { useProjectsStore, type Repo } from '@/stores/projects'
import { useRunsStore } from '@/stores/runs'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useSessionTabsStore } from '@/stores/sessionTabs'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import type { BoardRun } from '@/stores/flagBoard'
import { useConfirm } from '@/composables/useConfirm'
import { usePrompt } from '@/composables/usePrompt'
import { useToast } from '@/composables/useToast'
import { cardTitle } from '@/lib/flagBoard'
import { openSessionWindow } from '@/lib/sessionWindow'
import { IS_SESSION_WINDOW } from '@/lib/windowMode'

type Busy = { kind: 'refetching' | 'pinning' | 'protecting' | 'deleting' | 'forking'; id: string } | null

/** `onChanged`: reload the board after something that changes a card or removes it. */
export function useBoardRunActions(onChanged: () => void) {
  const { t } = useI18n()
  const router = useRouter()
  const projects = useProjectsStore()
  const runs = useRunsStore()
  const live = useLiveRunsStore()
  const sessionTabs = useSessionTabsStore()
  const workspaceTabs = useWorkspaceTabsStore()
  const { confirm } = useConfirm()
  const { prompt } = usePrompt()
  const { toast } = useToast()

  const menu = ref<{ run: BoardRun; x: number; y: number } | null>(null)
  const busy = ref<Busy>(null)
  // A Project's repos, for "Open on GitHub"; loaded when a menu opens on one of its issue / PR runs.
  const repos = reactive(new Map<string, Repo[]>())

  function openMenu(run: BoardRun, x: number, y: number) {
    menu.value = { run, x, y }
    if (run.repo_id && !repos.has(run.project_id)) {
      repos.set(run.project_id, [])
      projects.listRepos(run.project_id).then((list) => repos.set(run.project_id, list), () => repos.delete(run.project_id))
    }
  }

  function closeMenu() {
    menu.value = null
  }

  function githubUrl(run: BoardRun): string | null {
    if (run.run_type === 'session' || run.ref_number == null) return null
    const repo = repos.get(run.project_id)?.find((r) => r.id === run.repo_id)
    if (!repo?.github_owner || !repo?.github_repo) return null
    return `https://github.com/${repo.github_owner}/${repo.github_repo}/${run.run_type === 'analyze_issue' ? 'issues' : 'pull'}/${run.ref_number}`
  }

  function actionProps(run: BoardRun) {
    const is = (kind: NonNullable<Busy>['kind']) => busy.value?.kind === kind && busy.value.id === run.id
    return {
      run,
      isSessionWindow: IS_SESSION_WINDOW,
      canPopout: true,
      githubUrl: githubUrl(run),
      busy: {
        refetching: is('refetching'),
        pinning: is('pinning'),
        protecting: is('protecting'),
        deleting: is('deleting'),
        forking: is('forking'),
      },
    }
  }

  async function guarded(kind: NonNullable<Busy>['kind'], run: BoardRun, work: () => Promise<void>) {
    busy.value = { kind, id: run.id }
    try {
      await work()
    } catch (e) {
      toast.error(String(e))
    } finally {
      busy.value = null
    }
  }

  async function copyText(text: string, doneKey: string) {
    try {
      await navigator.clipboard.writeText(text)
      toast.success(t(doneKey))
    } catch (e) {
      toast.error(String(e))
    }
  }



  function openSession(run: BoardRun) {
    router.push({ name: 'project-run-detail', params: { projectId: run.project_id, runId: run.id } }).catch(() => {})
  }

  async function withLogPath(run: BoardRun, use: (path: string) => void | Promise<void>) {
    try {
      const path = await runs.getRunLogPath(run.id)
      if (!path) toast.info(t('run.noLogFileYet'))
      else await use(path)
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function rename(run: BoardRun) {
    const value = await prompt({
      title: t('run.rename'),
      initialValue: run.title ?? cardTitle(run, t),
      confirmLabel: t('common.save'),
      cancelLabel: t('common.cancel'),
      selectAll: true,
    })
    const title = value?.trim()
    if (value === null || title === (run.title ?? '')) return
    try {
      await runs.renameRun(run.id, title ?? '')
      toast.success(t('run.runRenamed'))
      onChanged()
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function fork(run: BoardRun) {
    const ok = await confirm({
      title: t('run.fork.confirmTitle'),
      message: t('run.fork.confirmMessage'),
      confirmLabel: t('run.fork.confirm'),
      cancelLabel: t('common.cancel'),
      variant: 'primary',
    })
    if (!ok) return
    await guarded('forking', run, async () => {
      const outcome = await runs.forkRun(run.id)
      toast.success(t(outcome.conversation ? 'run.fork.done' : 'run.fork.doneWithoutConversation'))
      onChanged()
    })
  }

  async function remove(run: BoardRun) {
    const ok = await confirm({
      title: t('run.deleteRunTitle'),
      message: t('run.deleteRunMessage', { label: cardTitle(run, t) }),
      confirmLabel: t('common.delete'),
    })
    if (!ok) return
    await guarded('deleting', run, async () => {
      await runs.deleteRun(run.id)
      live.discard(run.id)
      workspaceTabs.forgetRun(run.id)
      sessionTabs.forget([run.id])
      toast.success(t('run.runDeleted'))
      onChanged()
    })
  }

  function actionHandlers(run: BoardRun) {
    return {
      popout: () => void openSessionWindow(run.project_id, run.id),
      copyLogPath: () => withLogPath(run, (path) => copyText(path, 'run.logPathCopied')),
      revealLog: () => withLogPath(run, (path) => revealItemInDir(path)),
      openGithub: () => {
        const url = githubUrl(run)
        if (url) void openUrl(url)
      },
      refetch: () => guarded('refetching', run, async () => {
        await runs.refetchRun(run.id)
        onChanged()
      }),
      copyId: () => copyText(run.id, 'run.sessionIdCopied'),
      fork: () => fork(run),
      rename: () => rename(run),
      togglePin: () => guarded('pinning', run, async () => {
        const pin = !run.pinned
        await runs.setRunPinned(run.id, pin)
        toast.success(t(pin ? 'run.runPinned' : 'run.runUnpinned'))
        onChanged()
      }),
      toggleProtect: () => guarded('protecting', run, async () => {
        const protect = !run.protected
        await runs.setRunProtected(run.id, protect)
        toast.success(t(protect ? 'run.runProtected' : 'run.runUnprotected'))
        onChanged()
      }),
      // set_run_flag emits no event: the board copy of the run is stale until it reloads.
      flagChanged: () => onChanged(),
      delete: () => remove(run),
    }
  }

  return { menu, openMenu, closeMenu, openSession, actionProps, actionHandlers }
}
