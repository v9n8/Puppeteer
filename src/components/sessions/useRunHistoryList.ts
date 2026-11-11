// The left rail: fetch an issue / PR by URL, the session History list (search,
// density, row helpers) and the per-run actions shared with the header ⋯ menu.
// Part of the RunView split (see useRunSession.ts).
import { ref, computed, nextTick, watch, type ComponentPublicInstance } from 'vue'
import type { Repo } from '@/stores/projects'
import { runActivityAt, type RunRecord } from '@/stores/runs'
import { openUrl } from '@tauri-apps/plugin-opener'
import { GitPullRequest, Bug, Loader2, CircleDashed } from 'lucide-vue-next'
import { onClickOutside } from '@vueuse/core'
import {
  formatRunTimestamp, relativeAge, runStatusColorClass, runStatusIcon, shortModelId,
} from '@/lib/runDisplay'
import { openFileWindow } from '@/lib/fileWindow'
import { pageOfIndex, pageSlice } from '@/lib/listPaging'
import type { RunSessionCore } from './useRunSessionCore'
import type { RunConductor } from './useRunConductor'
import type { RunLog } from './useRunLog'

interface DisplayRun {
  run: RunRecord
}

/** What a pasted URL resolved to. `slug` is the repo it points at — `owner/repo`
 * for GitHub, the full project path for GitLab. */
interface ParsedRef {
  number: number
  kind: 'issue' | 'pr'
  slug: string
}

export function useRunHistoryList(core: RunSessionCore, conductor: RunConductor, log: RunLog) {
  const {
    t, router, runsStore, live, tabsStore, claudeStore, uiLayout, confirm, prompt, toast, projectId,
    project, isSessionWindow, activeRunId, engineOverride, loadedRunEngine, currentRunId, runLabel,
  } = core
  const {
    conductorIdsWithRunningWorker,
  } = conductor
  const {
    popoutSession, sessionListEl, viewingLogRunId, inputContent, inputContentRunId, inputContentError, scrollMemory,
    loadInputContent, clearHistoryView, forking, forkRun,
  } = log

  const repos = ref<Repo[]>([])

  // Fetch takes a URL and nothing else: the URL is the only input that states all
  // three things a fetch needs — which repo, issue vs PR, and the number. A bare
  // number is ambiguous on both the repo and the kind (GitHub shares one number
  // space between issues and PRs), so it is rejected rather than guessed at.
  const refInput = ref('')
  const detectedRef = ref<
    { kind: 'issue' | 'pr'; number: number; repoId: string; repoName: string } | null
  >(null)
  const fetching = ref(false)
  // The fetch form lives in a popover off the rail toolbar's Fetch icon, so the
  // run History gets the space. Closes on outside click (the trigger is ignored
  // so its own click can toggle) and after a successful fetch.
  const fetchOpen = ref(false)
  const fetchPanelEl = ref<HTMLElement | null>(null)
  const fetchTriggerEl = ref<HTMLElement | null>(null)
  onClickOutside(fetchPanelEl, () => { fetchOpen.value = false }, { ignore: [fetchTriggerEl] })

  // List density (set in Settings › General, persisted in the uiLayout store;
  // shared with the worker panel).
  const historyDensity = computed(() => uiLayout.listDensity)
  // Left rail view: session controls/history, or the project file tree.
  const leftTab = ref<'session' | 'files'>('session')

  const fetchError = ref<string | null>(null)
  const needsLinkedIssue = ref(false)
  const linkedIssueInput = ref('')

  // A run is "running" for display when it is streaming itself, or when any of
  // its workers is still streaming.
  function runIsRunning(run: RunRecord): boolean {
    if (run.status === 'running') return true
    return conductorIdsWithRunningWorker.value.has(run.id)
  }

  // History-row icon shows STATUS only (every session is a conductor, so there is
  // no type icon): a spinner while running, otherwise a status glyph tinted by
  // runIconColorClass. Issue/PR identity is a separate badge (runRefBadge).
  function runIcon(run: RunRecord) {
    if (runIsRunning(run)) return Loader2
    if (run.status === 'fetched' && run.run_type === 'session') return CircleDashed
    return runStatusIcon(run.status)
  }

  // Tooltip for the History-row status icon.
  function runStatusTitle(run: RunRecord): string {
    if (runIsRunning(run)) return t('common.status.running')
    if (run.status === 'fetched' && run.run_type === 'session') return t('common.status.draft')
    return t(`common.status.${run.status}`)
  }

  // Worker count per conductor, from the flat run list (workers carry
  // conductor_run_id). Drives the "⎇N" badge on History rows; 0 hides it.
  const workerCountByConductor = computed(() => {
    const counts = new Map<string, number>()
    for (const r of runsStore.runs) {
      if (!r.conductor_run_id) continue
      counts.set(r.conductor_run_id, (counts.get(r.conductor_run_id) ?? 0) + 1)
    }
    return counts
  })
  function runWorkerCount(run: RunRecord): number {
    return workerCountByConductor.value.get(run.id) ?? 0
  }

  // Issue/PR rows keep their identity as a small Bug / PR glyph before the title
  // (whose label already reads "Issue #N" / "PR #N"); fresh sessions have none.
  function runRefIcon(run: RunRecord) {
    if (run.run_type === 'session') return null
    return run.run_type === 'analyze_issue' ? Bug : GitPullRequest
  }

  // Compact model id for dense rows: drop a leading "claude-" (the account badge
  // already implies Claude); the full id stays in the tooltip.
  function runModelShort(run: RunRecord): string {
    return shortModelId(runModelLabel(run))
  }

  // "account · model" line for compact/dense rows ('' for an unstarted session).
  function runAccountModelLine(run: RunRecord, short = true): string {
    if (!runStarted(run)) return ''
    const model = short ? runModelShort(run) : runModelLabel(run)
    const acc = runClaudeAccountLabel(run)
    return acc ? `${acc} · ${model}` : model
  }

  // Relative age for compact/dense rows: "now", "5m", "2h", "3d", "2w", then
  // months/years. The full timestamp lives in the tooltip.
  function runRelativeTime(iso: string): string {
    return relativeAge(iso, t('run.relNow'))
  }

  // Dense rows fold everything not on the single line into the row tooltip.
  function runDenseTooltip(run: RunRecord): string {
    const parts = [runLabel(run)]
    const meta = runAccountModelLine(run, false)
    if (meta) parts.push(meta)
    parts.push(runTimestamp(runActivityAt(run)))
    return parts.join('\n')
  }

  // History row: tint the session icon by run status so the list conveys state
  // at a glance without a separate status pill. Mirrors StatusBadge's palette.
  // A freshly-created chat session ('fetched' + session) is a draft → stays muted.
  function runIconColorClass(run: RunRecord): string {
    if (runIsRunning(run)) return 'text-info'
    if (run.status === 'fetched' && run.run_type === 'session') return 'text-muted-foreground'
    return runStatusColorClass(run.status)
  }

  // History row: which Claude account this run used. Only Claude runs record an
  // account id (Codex has no per-run account), so we return a label just for
  // engine 'claude' with a matching, known account — otherwise '' (no badge).
  // Legacy runs with no run-level account fall back to the global ~/.claude, so
  // they intentionally show nothing rather than a guessed account.
  function runClaudeAccountLabel(run: RunRecord): string {
    if (run.engine !== 'claude' || !run.claude_account_id) return ''
    const acc = claudeStore.accounts.find((a) => a.id === run.claude_account_id)
    return acc ? acc.label : ''
  }

  // History row: the model this run runs on. Prefer the persisted per-run model
  // (resolved at start/resume); fall back to the engine name for runs that never
  // started or predate model persistence, so the badge is never empty.
  function runModelLabel(run: RunRecord): string {
    return run.model || run.engine
  }

  // A freshly created session that was never run (status 'fetched', `started_at`
  // still null) has no real model/account/activity yet — only `created_at`.
  // Showing the engine name as a model and a "last activity" that is really the
  // creation time reads as wrong. So for an unstarted session the meta block is
  // made `invisible` (kept in the layout, so the row stays the same height as a
  // full one) rather than removed. `started_at` is set the moment a run begins,
  // so it is the clean signal that there is real info worth showing.
  function runStarted(run: RunRecord): boolean {
    return !!run.started_at
  }

  // History row timestamp: date + time, so runs from the same day stay
  // distinguishable. Falls back to the raw string if the date can't be parsed.
  // Fed with `runActivityAt(run)` — the same value the list sorts by, so the
  // printed time always explains the row's position.
  function runTimestamp(iso: string): string {
    return formatRunTimestamp(iso)
  }

  // ── History search ────────────────────────────────────────────────────────
  // Free-text filter over the History list. Matching is done on what the row
  // actually shows (title/label, engine, status, issue/PR number) so whatever the
  // user reads in the list is also what they can type to find it again.
  const sessionSearch = ref('')
  const filteredRuns = computed(() => {
    const q = sessionSearch.value.trim().toLowerCase()
    if (!q) return runsStore.runs
    return runsStore.runs.filter((run) => {
      const haystack = [
        runLabel(run),
        run.title ?? '',
        run.engine,
        run.status,
        run.run_type,
        run.ref_number != null ? `#${run.ref_number}` : '',
      ]
        .join(' ')
        .toLowerCase()
      return haystack.includes(q)
    })
  })

  // ── History list ───────────────────────────────────────────────────────────
  // Worker sessions (run.conductor_run_id set) NEVER appear in the History list —
  // they live solely in the conductor's right-side worker panel. The History list
  // therefore shows only conductors and ordinary sessions/issues/PRs, each row
  // acting on its own run.id.

  // Flat view of `filteredRuns` with every worker filtered out (both workers
  // nested under a listed conductor and orphan workers whose conductor isn't
  // listed). Top-level order follows the store's sort.
  const displayRuns = computed<DisplayRun[]>(() => {
    const out: DisplayRun[] = []
    for (const run of filteredRuns.value) {
      if (run.conductor_run_id) continue
      out.push({ run })
    }
    return out
  })

  // One page of rows at a time (UX-62): a project can hold hundreds of sessions.
  // The page holding the open session follows it (click, deep link, a new
  // session at the top); a new search starts from page 1. Counts and "Clear all"
  // still work on the whole list.
  const historyPage = ref(1)
  const pagedRuns = computed(() => pageSlice(displayRuns.value, historyPage.value))
  watch(sessionSearch, () => { historyPage.value = 1 })
  // Also once the list has loaded: a deep link sets the session before its rows arrive.
  watch([currentRunId, () => runsStore.loading], ([id]) => {
    const page = pageOfIndex(displayRuns.value.findIndex((d) => d.run.id === id))
    if (page) historyPage.value = page
  }, { immediate: true })
  function goToHistoryPage(page: number) {
    historyPage.value = pageSlice(displayRuns.value, page).page
    sessionListEl.value?.scrollTo({ top: 0 })
  }

  // Ids of conductors the user has protected. Protection cascades to their
  // workers for "Clear all" (the bulk cleanup keeps them), so we badge those
  // workers too even though their own `protected` flag stays false — they remain
  // individually deletable.
  const protectedConductorIds = computed(
    () =>
      new Set(
        runsStore.runs
          .filter((r) => r.role === 'conductor' && r.protected)
          .map((r) => r.id),
      ),
  )
  function isProtectedByConductor(run: RunRecord): boolean {
    return (
      !run.protected &&
      run.conductor_run_id != null &&
      protectedConductorIds.value.has(run.conductor_run_id)
    )
  }

  // A run awaiting a permission / question response (front of its live queue), or
  // undefined. Drives the animated attention icon in the History list so the user
  // knows which run needs them without a floating toast.
  function pendingRequest(runId: string) {
    return live.get(runId)?.permissionQueue[0]
  }

  // Build the GitHub URL for an issue/PR run, or null if we can't resolve the repo.
  function runGithubUrl(run: RunRecord): string | null {
    if (run.run_type === 'session' || run.ref_number == null) return null
    const repo = repos.value.find(r => r.id === run.repo_id)
    if (!repo?.github_owner || !repo?.github_repo) return null
    const kind = run.run_type === 'analyze_issue' ? 'issues' : 'pull'
    return `https://github.com/${repo.github_owner}/${repo.github_repo}/${kind}/${run.ref_number}`
  }

  async function openRunInBrowser(run: RunRecord) {
    const url = runGithubUrl(run)
    if (url) await openUrl(url)
  }

  async function handleFetch(linkedIssueOverride?: number) {
    const target = detectedRef.value
    if (!target) {
      // fetchError may already name the unmapped repo — don't overwrite that.
      if (!fetchError.value) fetchError.value = t('run.refMustBeUrl')
      return
    }

    fetchError.value = null
    fetching.value = true
    try {
      let run: RunRecord
      if (target.kind === 'issue') {
        run = await runsStore.fetchIssue(projectId.value, target.repoId, target.number)
      } else {
        run = await runsStore.fetchPr(projectId.value, target.repoId, target.number, linkedIssueOverride)
      }
      await runsStore.fetchRuns(projectId.value)
      scrollMemory.flush()
      currentRunId.value = run.id
      clearHistoryView()
      needsLinkedIssue.value = false
      linkedIssueInput.value = ''
      fetchOpen.value = false
      await loadInputContent(run.id)
    } catch (e) {
      const msg = String(e)
      if (target.kind === 'pr' && msg.includes('NO_LINKED_ISSUE')) {
        needsLinkedIssue.value = true
        fetchError.value = t('run.prNoLinkedIssue')
      } else {
        fetchError.value = msg
      }
    } finally {
      fetching.value = false
    }
  }

  async function handleSubmitLinkedIssue() {
    const n = parseInt(linkedIssueInput.value.trim())
    if (isNaN(n) || n <= 0) { fetchError.value = t('run.enterValidIssueNumber'); return }
    await handleFetch(n)
  }

  // Re-fetch fresh PR/issue content from GitHub and overwrite this run's input
  // markdown IN PLACE. The existing AI result (output/session) is preserved so
  // the user can keep working on it with the refreshed context.
  async function handleRefetch(sourceRunId: string) {
    fetchError.value = null
    refetchingRunId.value = sourceRunId
    try {
      await runsStore.refetchRun(sourceRunId)
      // Force-refresh the input markdown shown in the view (output untouched).
      if (inputContentRunId.value === sourceRunId) {
        inputContentRunId.value = null
        await loadInputContent(sourceRunId)
      }
    } catch (e) {
      fetchError.value = String(e)
    } finally {
      refetchingRunId.value = null
    }
  }

  const deletingRunId = ref<string | null>(null)
  const refetchingRunId = ref<string | null>(null)
  const clearingAll = ref(false)
  const pinningRunId = ref<string | null>(null)
  const protectingRunId = ref<string | null>(null)
  // Which History row currently has its actions (⋯) menu open. Kept so the
  // trigger stays visible even when the pointer leaves the row while open.
  const actionsMenuRunId = ref<string | null>(null)

  // Inline rename state for the History list.
  const renamingRunId = ref<string | null>(null)
  const renameDraft = ref('')
  const renameInputEl = ref<HTMLInputElement | null>(null)
  // Only one rename overlay is rendered at a time (v-if on the matching run id),
  // so this ref always points at the active input (or null when none is open).
  function setRenameInputRef(el: Element | ComponentPublicInstance | null) {
    renameInputEl.value = (el as HTMLInputElement | null) ?? null
  }

  function startRename(run: RunRecord) {
    renamingRunId.value = run.id
    renameDraft.value = run.title ?? runLabel(run)
    nextTick(() => {
      renameInputEl.value?.focus()
      renameInputEl.value?.select()
    })
  }

  // Copy the absolute path of this run/session's log file to the clipboard.
  async function copyLogPath(run: RunRecord) {
    try {
      const path = await runsStore.getRunLogPath(run.id)
      if (!path) { toast.info(t('run.noLogFileYet')); return }
      await navigator.clipboard.writeText(path)
      toast.success(t('run.logPathCopied'))
    } catch (e) {
      toast.error(String(e))
    }
  }

  // Copy this run/session's Puppeteer run ID to the clipboard — the id the puppeteer MCP
  // (session_read) uses to load the session.
  async function copySessionId(run: RunRecord) {
    try {
      await navigator.clipboard.writeText(run.id)
      toast.success(t('run.sessionIdCopied'))
    } catch (e) {
      toast.error(String(e))
    }
  }

  // Open this run/session's log file in a standalone file viewer window.
  async function viewLogFile(run: RunRecord) {
    const p = project.value
    if (!p) return
    try {
      const path = await runsStore.getRunLogPath(run.id)
      if (!path) { toast.info(t('run.noLogFileYet')); return }
      await openFileWindow(p.path, path, null, run.id)
    } catch (e) {
      toast.error(String(e))
    }
  }

  // Write this run/session's conversation to a Markdown file (secrets redacted)
  // and open it in the file viewer.
  async function exportTranscript(run: RunRecord) {
    const p = project.value
    try {
      const path = await runsStore.exportTranscript(run.id)
      toast.success(t('run.transcriptExported', { path }))
      if (p) await openFileWindow(p.path, path, null, run.id)
    } catch (e) {
      toast.error(String(e))
    }
  }

  function cancelRename() {
    renamingRunId.value = null
    renameDraft.value = ''
  }

  async function commitRename(runId: string) {
    if (renamingRunId.value !== runId) return
    try {
      await applyRename(runId, renameDraft.value)
    } finally {
      cancelRename()
    }
  }

  /** Save a run's title; no change (or empty for a run without one) is a no-op. */
  async function applyRename(runId: string, value: string) {
    const title = value.trim()
    const run = runsStore.runs.find(r => r.id === runId)
    if (run && title === (run.title ?? '')) return
    try {
      await runsStore.renameRun(runId, title)
      toast.success(t('run.runRenamed'))
    } catch (err) {
      toast.error(String(err))
    }
  }

  // The header's ⋯ → Rename: the History row (and its inline field) may be off
  // screen — focus mode, scrolled away — so ask in a dialog instead.
  async function renameViaPrompt(run: RunRecord) {
    const value = await prompt({
      title: t('run.rename'),
      initialValue: run.title ?? runLabel(run),
      confirmLabel: t('common.save'),
      cancelLabel: t('common.cancel'),
      selectAll: true,
    })
    if (value !== null) await applyRename(run.id, value)
  }

  // ── Run actions (RunActionItems: History row ⋯ and header ⋯) ──────────────

  function runActionProps(run: RunRecord) {
    return {
      run,
      isSessionWindow,
      githubUrl: runGithubUrl(run),
      busy: {
        refetching: refetchingRunId.value === run.id,
        pinning: pinningRunId.value === run.id,
        protecting: protectingRunId.value === run.id,
        deleting: deletingRunId.value === run.id,
        forking: forking.value,
      },
    }
  }

  /** `rename` defaults to the History row's inline field. */
  function runActionHandlers(run: RunRecord, rename: (run: RunRecord) => void = startRename) {
    return {
      popout: () => popoutSession(run.id),
      openGithub: () => openRunInBrowser(run),
      refetch: () => handleRefetch(run.id),
      copyLogPath: () => copyLogPath(run),
      copyId: () => copySessionId(run),
      viewLog: () => viewLogFile(run),
      exportTranscript: () => exportTranscript(run),
      fork: () => forkRun(run),
      rename: () => rename(run),
      togglePin: () => handleTogglePin(run),
      toggleProtect: () => handleToggleProtect(run),
      delete: () => handleDeleteRun(run.id),
    }
  }

  async function handleTogglePin(run: RunRecord) {
    pinningRunId.value = run.id
    const willPin = !run.pinned
    try {
      await runsStore.setRunPinned(run.id, willPin)
      toast.success(willPin ? t('run.runPinned') : t('run.runUnpinned'))
    } catch (err) {
      toast.error(String(err))
    } finally {
      pinningRunId.value = null
    }
  }

  async function handleToggleProtect(run: RunRecord) {
    protectingRunId.value = run.id
    const willProtect = !run.protected
    try {
      await runsStore.setRunProtected(run.id, willProtect)
      toast.success(willProtect ? t('run.runProtected') : t('run.runUnprotected'))
    } catch (err) {
      toast.error(String(err))
    } finally {
      protectingRunId.value = null
    }
  }

  function clearActiveRunState() {
    if (currentRunId.value) live.discard(currentRunId.value)
    currentRunId.value = null
    // A fresh run starts on the global default (empty = follow settings).
    engineOverride.value = ''
    loadedRunEngine.value = ''
    clearHistoryView()
    inputContent.value = ''
    inputContentRunId.value = null
    inputContentError.value = null
  }

  async function handleDeleteRun(runId: string) {
    const run = runsStore.runs.find(r => r.id === runId)
    const label = run ? runLabel(run) : t('run.thisRun')
    if (!(await confirm({
      title: t('run.deleteRunTitle'),
      message: t('run.deleteRunMessage', { label }),
      confirmLabel: t('common.delete'),
    }))) return
    deletingRunId.value = runId
    try {
      await runsStore.deleteRun(runId)
      if (currentRunId.value === runId || viewingLogRunId.value === runId) {
        clearActiveRunState()
      } else {
        live.discard(runId)
      }
      // Forget the deleted run so its project tab won't resume onto it, and
      // navigate away from a deep-linked deleted run so the URL stays consistent.
      tabsStore.forgetRun(runId)
      scrollMemory.forget(runId)
      if (activeRunId.value === runId) {
        router.replace(`/projects/${projectId.value}`)
      }
      toast.success(t('run.runDeleted'))
    } catch (err) {
      toast.error(String(err))
    } finally {
      deletingRunId.value = null
    }
  }

  async function handleClearAllRuns() {
    if (runsStore.runs.length === 0) return
    if (!(await confirm({
      title: t('run.deleteAllRunsTitle'),
      message: t('run.deleteAllRunsMessage'),
      confirmLabel: t('run.deleteAll'),
    }))) return
    clearingAll.value = true
    try {
      await runsStore.deleteAllRuns(projectId.value)
      clearActiveRunState()
      if (activeRunId.value) {
        router.replace(`/projects/${projectId.value}`)
      }
      toast.success(t('run.allRunsDeleted'))
    } catch (err) {
      toast.error(String(err))
    } finally {
      clearingAll.value = false
    }
  }


  /** Parse an issue/PR/MR URL. Anything else — including a bare number — is null. */
  function parseIssueOrPrUrl(input: string): ParsedRef | null {
    const val = input.trim()

    // GitLab puts the ref under a `/-/` separator, and the project path may have
    // any number of subgroup segments: host/group/sub/project/-/merge_requests/42
    const gitlab = val.match(/^(?:https?:\/\/)?[^/\s]+\/(.+?)\/-\/(merge_requests|issues)\/(\d+)/i)
    if (gitlab) {
      return {
        number: parseInt(gitlab[3]),
        kind: gitlab[2].toLowerCase() === 'merge_requests' ? 'pr' : 'issue',
        slug: gitlab[1],
      }
    }

    // GitHub (and GHE — the host is ignored): host/owner/repo/pull/123
    const github = val.match(/^(?:https?:\/\/)?[^/\s]+\/([^/\s]+\/[^/\s]+)\/(pull|issues)\/(\d+)/i)
    if (github) {
      return {
        number: parseInt(github[3]),
        kind: github[2].toLowerCase() === 'pull' ? 'pr' : 'issue',
        slug: github[1],
      }
    }

    return null
  }

  /** The repo slug as the pasted URL would spell it, for matching. */
  function repoSlug(r: Repo): string | null {
    if (r.provider === 'gitlab') return r.gitlab_project_path
    if (r.github_owner && r.github_repo) return `${r.github_owner}/${r.github_repo}`
    return null
  }

  function handleRefInput(val: string) {
    refInput.value = val
    needsLinkedIssue.value = false
    linkedIssueInput.value = ''
    fetchError.value = null
    detectedRef.value = null
    if (!val.trim()) return

    // Stay quiet while a URL is still half-typed — `handleFetch` reports the
    // "not a URL" case on submit. An unmapped repo, by contrast, comes from a
    // fully parsed URL, so it is safe to flag live.
    const parsed = parseIssueOrPrUrl(val)
    if (!parsed) return
    // The URL names its own repo — resolve it rather than trusting a selection,
    // so #N is never pulled from a different repo than the one that was pasted.
    const match = repos.value.find(
      r => repoSlug(r)?.toLowerCase() === parsed.slug.toLowerCase(),
    )
    if (!match) {
      fetchError.value = t('run.repoNotInProject', { slug: parsed.slug })
      return
    }
    detectedRef.value = {
      kind: parsed.kind,
      number: parsed.number,
      repoId: match.id,
      repoName: match.name,
    }
  }

  return {
    repos, refInput, detectedRef, fetching, fetchOpen, fetchPanelEl, fetchTriggerEl, historyDensity,
    leftTab, fetchError, needsLinkedIssue, linkedIssueInput, runIsRunning,
    runIcon, runStatusTitle, runWorkerCount, runRefIcon, runAccountModelLine, runRelativeTime,
    runDenseTooltip, runIconColorClass, runClaudeAccountLabel, runModelLabel, runStarted,
    runTimestamp, sessionSearch, filteredRuns, displayRuns, pagedRuns, goToHistoryPage, isProtectedByConductor, pendingRequest,
    runGithubUrl, handleFetch, handleSubmitLinkedIssue, clearingAll, actionsMenuRunId, renamingRunId,
    renameDraft, setRenameInputRef, cancelRename, commitRename, renameViaPrompt, runActionProps,
    runActionHandlers, handleClearAllRuns, handleRefInput,
  }
}

export type RunHistoryList = ReturnType<typeof useRunHistoryList>
