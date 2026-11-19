// What the session info bar shows about the project: its git accounts, VPS servers and
// cloud accounts (the Connections menu), the session's linked items, quick capture and "open the
// project in …". Part of the RunView split (see useRunSession.ts).
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ProjectServer } from '@/stores/servers'
import { invoke } from '@/lib/tauri'
import type { ConnectionKind } from '@/lib/sessionInfoBar'
import { useRunLinkedItems } from '@/composables/useRunLinkedItems'
import { openItemCreateWindow, type ItemKind } from '@/lib/itemWindow'
import type { RunSessionCore } from './useRunSessionCore'

interface RunGithubAccounts { default: string | null; pinned: boolean; enabled: string[] }

/** One line of the info bar's Connections menu. */
export interface ConnectionRow {
  key: string
  label: string
  /** `user@host` of a server, the region of an AWS account, the subscription of an Azure one. */
  sub?: string
  provider?: 'github' | 'gitlab'
  isDefault?: boolean
  isReview?: boolean
  /** A server's connection state; `offline` is written out in the menu. */
  status?: 'online' | 'offline' | 'unknown'
  title?: string
}
export interface ConnectionGroup { id: ConnectionKind; rows: ConnectionRow[] }

export function useRunHeaderInfo(core: RunSessionCore) {
  const { t } = useI18n()
  const {
    ghStore, glStore, serversStore, awsStore, azureStore, projectId,
    project, currentRunId,
  } = core

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

  // VPS servers mapped to this project — pre-wired for transparent SSH on each run.
  // Loaded per-project (list_project_servers).
  const projectServers = ref<ProjectServer[]>([])
  async function loadProjectServers() {
    try {
      projectServers.value = await serversStore.listForProject(projectId.value)
    } catch {
      projectServers.value = []
    }
  }

  // What the session can use, grouped for the info bar's Connections menu: the
  // git accounts (default first, then the other enabled ones), the project's VPS
  // servers, and the AWS / Azure accounts (default first).
  const connectionGroups = computed<ConnectionGroup[]>(() => {
    const git: ConnectionRow[] = []
    const ghDefault = runGithubAccounts.value?.default ?? project.value?.github_account_id
    const ghEnabled = runGithubAccounts.value?.enabled ?? project.value?.github_account_ids ?? []
    const ghIds = [ghDefault, ...ghEnabled].filter((id, i, all): id is string => !!id && all.indexOf(id) === i)
    for (const id of ghIds) {
      const acc = ghStore.accounts.find(a => a.id === id)
      if (!acc) continue
      const name = acc.username ? `@${acc.username}` : acc.label
      const isDefault = id === ghDefault
      const review = isDefault && !!runGithubAccounts.value?.pinned
      git.push({
        key: `github-${id}`,
        provider: 'github',
        label: name,
        isDefault: isDefault && !review,
        isReview: review,
        title: isDefault
          ? t(review ? 'runHeader.tooltip.githubReview' : 'runHeader.tooltip.githubDefault', { name })
          : undefined,
      })
    }
    const glDefault = project.value?.gitlab_account_id
    const glIds = [glDefault, ...(project.value?.gitlab_account_ids ?? [])]
      .filter((id, i, all): id is string => !!id && all.indexOf(id) === i)
    for (const id of glIds) {
      const acc = glStore.accounts.find(a => a.id === id)
      if (!acc) continue
      const isDefault = id === glDefault
      const who = `${acc.label}${acc.username ? ` (@${acc.username})` : ''}${acc.host ? t('runHeader.tooltip.onHost', { host: acc.host }) : ''}`
      git.push({
        key: `gitlab-${id}`,
        provider: 'gitlab',
        label: acc.username ? `@${acc.username}` : acc.label,
        isDefault,
        title: isDefault ? t('runHeader.tooltip.gitlabDefault', { who }) : undefined,
      })
    }

    const servers: ConnectionRow[] = projectServers.value.map((s) => ({
      key: `${s.id}-${s.role}`,
      label: s.label,
      sub: `${s.username}@${s.host}`,
      status: s.status === 'online' ? 'online' : s.status === 'offline' ? 'offline' : 'unknown',
      title: `${s.role}: ${s.username}@${s.host}:${s.port} · ${t('runHeader.tooltip.serverAuth', { auth: s.auth_method === 'key' ? 'key' : 'ssh-agent' })}`,
    }))

    const awsDefault = project.value?.aws_account_id
    const aws: ConnectionRow[] = [awsDefault, ...(project.value?.aws_account_ids ?? [])]
      .filter((id, i, all): id is string => !!id && all.indexOf(id) === i)
      .flatMap((id) => {
        const acc = awsStore.accounts.find(a => a.id === id)
        if (!acc) return []
        const parts = [id === awsDefault ? t('runHeader.tooltip.awsDefault', { label: acc.label }) : acc.label]
        if (acc.account_id) parts.push(t('runHeader.tooltip.account', { id: acc.account_id }))
        return [{ key: `aws-${id}`, label: acc.label, sub: acc.region, title: parts.join(' · ') + (acc.arn ? `\n${acc.arn}` : '') }]
      })

    const azureDefault = project.value?.azure_account_id
    const azure: ConnectionRow[] = [azureDefault, ...(project.value?.azure_account_ids ?? [])]
      .filter((id, i, all): id is string => !!id && all.indexOf(id) === i)
      .flatMap((id) => {
        const acc = azureStore.accounts.find(a => a.id === id)
        if (!acc) return []
        const parts = [id === azureDefault ? t('runHeader.tooltip.azureDefault', { label: acc.label }) : acc.label]
        if (acc.subscription_name) parts.push(t('runHeader.tooltip.subscription', { name: acc.subscription_name }))
        if (acc.tenant_id) parts.push(t('runHeader.tooltip.tenant', { id: acc.tenant_id }))
        return [{ key: `azure-${id}`, label: acc.label, sub: acc.subscription_name ?? undefined, title: parts.join(' · ') }]
      })

    return [
      { id: 'git' as const, rows: git },
      { id: 'servers' as const, rows: servers },
      { id: 'aws' as const, rows: aws },
      { id: 'azure' as const, rows: azure },
    ].filter(g => g.rows.length > 0)
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

  // Slack threads / todos / notes linked to this session, for the info bar's Links menu.
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

  return {
    connectionGroups, loadProjectServers, captureContext, openQuickCapture, linked,
  }
}

export type RunHeaderInfo = ReturnType<typeof useRunHeaderInfo>
