<script setup lang="ts">
// Configuration health of a project (V3-48): one short list at the top of the
// Overview. Each row reads real data; a row with a problem links to where it is
// fixed, and the conflict row can scan the disk again.
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, CheckCircle2, Loader2 } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { useMcpHealth } from '@/composables/useMcpHealth'
import { useToast } from '@/composables/useToast'
import { composerDefaults } from '@/lib/composerChips'
import { healthRows, type HealthLine, type HealthRow, type Source } from '@/lib/projectHealth'
import type { ProjectSectionId } from '@/lib/projectSections'
import { scanMessage, scanOutcome } from '@/lib/syncConflicts'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { useProjectsStore, type Project, type Repo } from '@/stores/projects'
import { useServersStore } from '@/stores/servers'

const props = defineProps<{ project: Project }>()
const emit = defineEmits<{ open: [section: ProjectSectionId] }>()

const { t } = useI18n()
const { toast } = useToast()
const store = useProjectsStore()
const appSettings = useAppSettingsStore()
const claudeStore = useClaudeAccountsStore()
const codexStore = useCodexAccountsStore()
const serversStore = useServersStore()

const projectId = computed(() => props.project.id)

// --- Sources: each loads on its own, so one failing leaves the other rows alone ---
const repos = ref<Source<string[]>>({ state: 'loading' })
const servers = ref<Source<string[]>>({ state: 'loading' })

function repoName(repo: Repo): string {
  if (repo.provider === 'gitlab') return repo.gitlab_project_path || repo.path
  return repo.github_owner && repo.github_repo ? `${repo.github_owner}/${repo.github_repo}` : repo.path
}

async function loadRepos(id: string) {
  let next: Source<string[]>
  try {
    next = { state: 'ready', value: (await store.listRepos(id)).map(repoName) }
  } catch (e) {
    next = { state: 'error', error: String(e) }
  }
  // A slower answer for a project the user already left is dropped.
  if (projectId.value === id) repos.value = next
}

async function loadServers(id: string) {
  let next: Source<string[]>
  try {
    next = { state: 'ready', value: (await serversStore.listForProject(id)).map((s) => s.label) }
  } catch (e) {
    next = { state: 'error', error: String(e) }
  }
  if (projectId.value === id) servers.value = next
}

watch(projectId, (id) => {
  repos.value = { state: 'loading' }
  servers.value = { state: 'loading' }
  void loadRepos(id)
  void loadServers(id)
}, { immediate: true })

// The Repository block below adds and removes repos through the store; follow it.
const stopActions = store.$onAction(({ name, after }) => {
  if (name === 'addRepo' || name === 'removeRepo') after(() => { void loadRepos(projectId.value) })
})
onBeforeUnmount(stopActions)

// --- Engine account: the one new sessions of this project start with ---
const accountsReady = ref(false)
const accountsError = ref<string | null>(null)
async function loadAccounts() {
  await Promise.all([
    appSettings.ensureLoaded().catch(() => {}),
    claudeStore.accounts.length ? Promise.resolve() : claudeStore.fetch(),
    codexStore.accounts.length ? Promise.resolve() : codexStore.fetch(),
  ])
  accountsError.value = claudeStore.error ?? codexStore.error
  accountsReady.value = true
}
void loadAccounts()

const engine = computed<'claude' | 'codex'>(() =>
  composerDefaults({ settings: appSettings.settings, project: props.project.session_defaults }).engine === 'codex' ? 'codex' : 'claude')

const account = computed<Source<string | null>>(() => {
  if (!accountsReady.value) return { state: 'loading' }
  if (accountsError.value) return { state: 'error', error: accountsError.value }
  const list = engine.value === 'codex' ? codexStore.accounts : claudeStore.accounts
  const linkedId = engine.value === 'codex' ? props.project.codex_account_id : props.project.claude_account_id
  const found = list.find((a) => a.id === linkedId) ?? list.find((a) => a.is_default)
  return { state: 'ready', value: found ? found.label : null }
})

// --- MCP tests and conflicts ---
const { mcp } = useMcpHealth(projectId)
const conflicts = computed(() => store.projectConflicts[props.project.id] ?? [])

const rows = computed(() => healthRows({
  repos: repos.value,
  engine: engine.value,
  account: account.value,
  mcp: mcp.value,
  conflicts: conflicts.value,
  servers: servers.value,
}))

const rechecking = ref(false)
async function recheck() {
  rechecking.value = true
  try {
    const report = await store.checkProjectConflicts(props.project.id)
    toast.success(scanMessage(scanOutcome(report), (key, params) => t(key, params ?? {})))
  } catch (e) {
    toast.error(String(e))
  } finally {
    rechecking.value = false
  }
}

function openLink(row: HealthRow) {
  const link = row.link
  if (!link) return
  // The Repository block is on this page: scroll to it instead of changing section.
  if (link.target === 'repos') {
    document.getElementById('project-repositories')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    return
  }
  emit('open', link.target)
}

const lineText = (line: HealthLine) => (line.key ? t(line.key, line.params ?? {}) : (line.text ?? ''))
const LINK_CLASS = 'inline-flex min-h-7 shrink-0 cursor-pointer items-center rounded-md px-2 text-[13px] text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'
</script>

<template>
  <section class="mb-1 border-b border-border/60 pb-4" :aria-label="t('projectDetail.health.title')">
    <h2 class="mb-2 text-2xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('projectDetail.health.title') }}</h2>
    <ul class="divide-y divide-border/50 rounded-lg border border-border bg-card">
      <li v-for="row in rows" :key="row.key" class="flex flex-wrap items-start gap-x-3 gap-y-1 px-3 py-2.5">
        <Loader2 v-if="row.tone === 'busy'" class="mt-0.5 h-4 w-4 shrink-0 animate-spin text-muted-foreground" :stroke-width="1.75" aria-hidden="true" />
        <AlertTriangle v-else-if="row.tone === 'warn'" class="mt-0.5 h-4 w-4 shrink-0 text-warning" :stroke-width="1.75" :aria-label="t('projectDetail.health.needsFix')" />
        <CheckCircle2 v-else class="mt-0.5 h-4 w-4 shrink-0 text-success" :stroke-width="1.75" :aria-label="t('projectDetail.health.fine')" />
        <div class="min-w-0 flex-1 basis-56">
          <button
            v-if="row.key === 'conflicts' && row.link"
            type="button"
            class="cursor-pointer break-words rounded text-left text-[13px] font-medium hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            @click="openLink(row)"
          >{{ t(row.title.key, row.title.params ?? {}) }}</button>
          <p v-else class="break-words text-[13px] font-medium">{{ t(row.title.key, row.title.params ?? {}) }}</p>
          <p v-for="(line, i) in row.subs" :key="i" class="mt-0.5 break-words text-[13px] text-muted-foreground">{{ lineText(line) }}</p>
        </div>
        <div class="flex shrink-0 flex-wrap items-center gap-1">
          <button v-if="row.link" type="button" :class="LINK_CLASS" @click="openLink(row)">{{ t(row.link.labelKey) }}</button>
          <Button
            v-if="row.action === 'recheck'"
            variant="outline"
            size="xs"
            class="min-h-7 text-[13px]"
            :disabled="rechecking"
            :title="t('projectDetail.checkAgainTitle')"
            @click="recheck"
          >{{ rechecking ? t('projectDetail.health.rechecking') : t('projectDetail.checkAgain') }}</Button>
        </div>
      </li>
    </ul>
  </section>
</template>
