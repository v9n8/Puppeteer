// What the run header shows about the project: its git accounts, VPS servers and
// cloud accounts, the session's linked items, quick capture and "open the
// project in …". Part of the RunView split (see useRunSession.ts).
import { ref, computed, watch } from 'vue'
import type { ProjectServer } from '@/stores/servers'
import { invoke } from '@/lib/tauri'
import { Github, Gitlab } from 'lucide-vue-next'
import { useRunLinkedItems } from '@/composables/useRunLinkedItems'
import { openItemCreateWindow, type ItemKind } from '@/lib/itemWindow'
import type { RunSessionCore } from './useRunSessionCore'

interface RunGithubAccounts { default: string | null; pinned: boolean; enabled: string[] }

type ServerTone = 'success' | 'error' | 'neutral'
type ServerChip = { key: string; label: string; tone: ServerTone; title: string }

export function useRunHeaderInfo(core: RunSessionCore) {
  const {
    projectStore, runsStore, ghStore, glStore, serversStore, awsStore, azureStore, toast, projectId,
    project, currentRunId,
  } = core

  // GĐ6 (AC3): the project's active git account(s). Shows one badge per linked
  // provider (GitHub first, then GitLab) so both attached accounts are visible.
  // GitHub accounts the open session uses: `default` for commands that name no
  // account (a pinned PR-inbox review differs from the project default) and every
  // `enabled` account the agent may pick. Null until resolved.
  const runGithubAccounts = ref<RunGithubAccounts | null>(null)
  let runGithubAccountsSeq = 0
  async function loadRunGithubAccounts() {
    // Drop late replies so a fast session/project switch never shows the old
    // session's account.
    const seq = ++runGithubAccountsSeq
    let next: RunGithubAccounts | null = null
    try {
      next = await invoke<RunGithubAccounts>('get_run_github_accounts', {
        projectId: projectId.value,
        runId: currentRunId.value,
      })
    } catch {
      next = null
    }
    if (seq === runGithubAccountsSeq) runGithubAccounts.value = next
  }

  const activeAccounts = computed(() => {
    // Tone per provider is kept in sync with the Projects list: GitHub=info,
    // GitLab=warning (server chips use status tones, AWS uses primary).
    const badges: { key: string; icon: typeof Github; tone: 'info' | 'warning'; username: string; title: string; tag?: string }[] = []
    // One chip per provider: the default account of this session. Other enabled
    // accounts (the agent may pick them per command) show as "+N" and in the tooltip.
    const ghLabel = (id: string | null | undefined) => {
      const acc = id ? ghStore.accounts.find(a => a.id === id) : undefined
      return acc ? (acc.username ? `@${acc.username}` : acc.label) : null
    }
    const ghId = runGithubAccounts.value?.default ?? project.value?.github_account_id
    const ghName = ghLabel(ghId)
    if (ghId && ghName) {
      const others = (runGithubAccounts.value?.enabled ?? project.value?.github_account_ids ?? [])
        .filter(id => id !== ghId)
        .map(ghLabel)
        .filter((n): n is string => !!n)
      const lines = [runGithubAccounts.value?.pinned ? `Review session — GitHub runs as ${ghName}` : `GitHub account: ${ghName} (default)`]
      if (others.length) lines.push(`Also enabled: ${others.join(', ')}`)
      badges.push({
        key: 'github',
        icon: Github,
        tone: 'info',
        username: ghName,
        title: lines.join('\n'),
        tag: runGithubAccounts.value?.pinned ? 'review' : others.length ? `+${others.length}` : undefined,
      })
    }
    const glId = project.value?.gitlab_account_id
    if (glId) {
      const acc = glStore.accounts.find(a => a.id === glId)
      if (acc) {
        const others = (project.value?.gitlab_account_ids ?? [])
          .filter(id => id !== glId)
          .map(id => glStore.accounts.find(a => a.id === id))
          .filter((a): a is NonNullable<typeof a> => !!a)
          .map(a => (a.username ? `@${a.username}` : a.label))
        const lines = [`GitLab account: ${acc.label}${acc.username ? ` (@${acc.username})` : ''}${acc.host ? ` on ${acc.host}` : ''} (default)`]
        if (others.length) lines.push(`Also enabled: ${others.join(', ')}`)
        badges.push({
          key: 'gitlab',
          icon: Gitlab,
          tone: 'warning',
          username: acc.username ? `@${acc.username}` : acc.label,
          title: lines.join('\n'),
          tag: others.length ? `+${others.length}` : undefined,
        })
      }
    }
    return badges
  })

  // VPS servers mapped to this project — pre-wired for transparent SSH on each run.
  // Loaded per-project (list_project_servers); the header surfaces which hosts the
  // AI can reach, mirroring the git-account badges above.
  const projectServers = ref<ProjectServer[]>([])
  async function loadProjectServers() {
    try {
      projectServers.value = await serversStore.listForProject(projectId.value)
    } catch {
      projectServers.value = []
    }
  }

  // Max distinct server chips before collapsing the rest into a single "+N" chip,
  // keeping the (narrow) header from overflowing when many servers are mapped.
  const MAX_SERVER_CHIPS = 2
  // Tone follows connection status; the shared Badge renders the solid high-contrast fill.
  const serverChips = computed<ServerChip[]>(() =>
    projectServers.value.map((s) => {
      const tone: ServerTone = s.status === 'online' ? 'success' : s.status === 'offline' ? 'error' : 'neutral'
      const auth = s.auth_method === 'key' ? 'key' : 'ssh-agent'
      const status = s.status ? ` · ${s.status}` : ''
      return {
        key: `${s.id}-${s.role}`,
        label: s.label,
        tone,
        title: `${s.role}: ${s.username}@${s.host}:${s.port} · auth ${auth}${status}`,
      }
    }),
  )
  const visibleServerChips = computed(() => serverChips.value.slice(0, MAX_SERVER_CHIPS))
  const hiddenServerChips = computed(() => serverChips.value.slice(MAX_SERVER_CHIPS))
  const hiddenServerTitle = computed(() =>
    hiddenServerChips.value.map((c) => c.label).join('\n'),
  )

  // Labels of the enabled cloud accounts other than the default, for the "+N" tag
  // and the "Also enabled" tooltip line (like the GitHub / GitLab chips).
  function otherCloudLabels(ids: string[] | undefined, defaultId: string, accounts: { id: string; label: string }[]) {
    return (ids ?? [])
      .filter((id) => id !== defaultId)
      .map((id) => accounts.find((a) => a.id === id)?.label)
      .filter((l): l is string => !!l)
  }

  // The project's default AWS account (credentials brokered per run). Label + region
  // on the chip; account id + ARN and other enabled accounts in the tooltip.
  const awsChip = computed(() => {
    const id = project.value?.aws_account_id
    if (!id) return null
    const acc = awsStore.accounts.find((a) => a.id === id)
    if (!acc) return null
    const parts = [`AWS: ${acc.label} (default)`]
    if (acc.account_id) parts.push(`account ${acc.account_id}`)
    parts.push(`region ${acc.region}`)
    const others = otherCloudLabels(project.value?.aws_account_ids, id, awsStore.accounts)
    const title = parts.join(' · ') + (acc.arn ? `\n${acc.arn}` : '')
      + (others.length ? `\nAlso enabled: ${others.join(', ')}` : '')
    return { label: acc.label, region: acc.region, title, tag: others.length ? `+${others.length}` : undefined }
  })

  // The project's default Azure account (isolated az login per account). Label +
  // subscription name on the chip; subscription/tenant ids + user and other
  // enabled accounts in the tooltip.
  const azureChip = computed(() => {
    const id = project.value?.azure_account_id
    if (!id) return null
    const acc = azureStore.accounts.find((a) => a.id === id)
    if (!acc) return null
    const parts = [`Azure: ${acc.label} (default)`]
    if (acc.subscription_name) parts.push(`subscription ${acc.subscription_name}`)
    if (acc.subscription_id) parts.push(acc.subscription_id)
    if (acc.tenant_id) parts.push(`tenant ${acc.tenant_id}`)
    const others = otherCloudLabels(project.value?.azure_account_ids, id, azureStore.accounts)
    const title = parts.join(' · ') + (acc.user_name ? `\n${acc.user_name}` : '')
      + (others.length ? `\nAlso enabled: ${others.join(', ')}` : '')
    return { label: acc.label, subscription: acc.subscription_name ?? '', title, tag: others.length ? `+${others.length}` : undefined }
  })

  // ── Quick capture from a run ────────────────────────────────────────────────
  // Jotting a thought down mid-run must not cost the user this screen: the capture
  // opens in the standalone item window (its own OS window, so no route change and
  // no overlay — the stream keeps its scroll and every open panel survives),
  // pre-filed under this project and run.
  function captureContext() {
    return { projectId: projectId.value, runId: currentRunId.value }
  }

  function openQuickCapture(kind: ItemKind) {
    openItemCreateWindow(kind, captureContext())
  }

  // Slack threads / todos / notes linked to this session, for the header menus.
  const linked = useRunLinkedItems(currentRunId)

  // Keep the server chips in sync if the active project changes without a remount.
  watch(projectId, () => {
    loadProjectServers()
  })

  // Re-resolve the session's GitHub accounts when the project, the open session,
  // or the project's account links change.
  watch(
    [projectId, currentRunId, () => project.value?.github_account_id, () => project.value?.github_account_ids.join(',')],
    () => { void loadRunGithubAccounts() },
    { immediate: true },
  )

  async function handleOpenInVscode() {
    if (!project.value) return
    try {
      // Reveal the most recently loaded issue/PR markdown alongside the project folder.
      const run = currentRunId.value
        ? runsStore.runs.find(r => r.id === currentRunId.value)
        : null
      await projectStore.openInVscode(project.value.path, run?.input_path ?? undefined)
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function handleOpenInFolder() {
    if (!project.value) return
    try {
      await projectStore.openInFolder(project.value.path)
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function handleOpenInTerminal() {
    if (!project.value) return
    try {
      const settings = await invoke<{ terminal_app: string }>('get_settings')
      await projectStore.openInTerminal(project.value.path, settings.terminal_app)
    } catch (e) {
      toast.error(String(e))
    }
  }

  return {
    activeAccounts, loadProjectServers, visibleServerChips, hiddenServerChips, hiddenServerTitle,
    awsChip, azureChip, captureContext, openQuickCapture, linked, handleOpenInVscode,
    handleOpenInFolder, handleOpenInTerminal,
  }
}

export type RunHeaderInfo = ReturnType<typeof useRunHeaderInfo>
