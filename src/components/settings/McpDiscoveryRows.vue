<script setup lang="ts">
// Rows of one discovery section (Settings' "Global connections", a project's
// "Project and global connections"): the MCP servers found outside Puppeteer,
// each with its source, transport, whether runs load it, its state and what to
// do about it — Import (not managed), Open the managed copy, Sync (changed since
// the import), or why nothing can be done (blocked, unreadable). claude.ai
// connectors come after, one group per Claude profile, with the profile's login
// labels (C1 / C2 / C4) and a failed connector check (AC-015); a connector whose
// run key another server already uses carries C3. Names are untrusted text from
// other tools' files: rendered as text only.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, ArrowRight, Download, RefreshCw } from 'lucide-vue-next'
import { Badge, Button } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { errorText } from '@/lib/featureGate'
import {
  conflictTone,
  outcomeCounts,
  rowAction,
  rowConflicts,
  splitRows,
  type ClaudeProfile,
  type ConflictCode,
  type ConnectorGroup,
  type DiscoveryProblem,
  type DiscoveryRow,
  type ProfileRef,
  type RowState,
} from '@/lib/mcpDiscovery'
import { useMcpDiscoveryStore } from '@/stores/mcpDiscovery'

const props = defineProps<{
  rows: DiscoveryRow[]
  /** Claude profiles to show a connector group for, even an empty one; [] where no connector belongs. */
  profiles: ClaudeProfile[]
  problems: DiscoveryProblem[]
  /** Connector checks still running: an empty profile says so instead of "none". */
  probing: boolean
}>()

const emit = defineEmits<{
  open: [serverId: string]
  /** An import or sync changed the MCP list (a host with its own copy of the list reloads it). */
  changed: []
}>()

const { t, te } = useI18n()
const { toast } = useToast()
const store = useMcpDiscoveryStore()

const split = computed(() => splitRows(props.rows, props.profiles, props.problems))

/** Config rows first (no header when they are alone), then one block per Claude profile. */
const blocks = computed(() => [
  ...(split.value.servers.length ? [{ key: 'config', group: null as ConnectorGroup | null, rows: split.value.servers }] : []),
  ...split.value.connectors.map((g) => ({ key: `profile:${g.profile.ref}`, group: g as ConnectorGroup | null, rows: g.rows })),
])

const STATE_TONE: Record<RowState, 'neutral' | 'success' | 'warning' | 'error'> = {
  not_managed: 'warning',
  managed: 'success',
  changed: 'warning',
  blocked: 'neutral',
  unreadable: 'error',
}

function profileName(p: ProfileRef): string {
  return p.label ?? t('mcp.discovery.globalProfile')
}

function profileNameOf(ref: string): string {
  const p = props.profiles.find((x) => x.ref === ref)
  return p ? profileName(p) : ref
}

function conflictTitle(code: ConflictCode, group: ConnectorGroup | null, row: DiscoveryRow | null): string {
  if (code === 'C3') return t('mcp.discovery.conflictTitle.C3', { key: row?.importName ?? '' })
  const others = props.profiles.find((p) => p.ref === group?.profile.ref)?.sameLoginAs ?? []
  return t(`mcp.discovery.conflictTitle.${code}`, { others: others.map(profileNameOf).join(', ') })
}

function transportText(row: DiscoveryRow): string {
  return row.transport === 'connector' || row.transport === 'unknown'
    ? t(`mcp.discovery.transport.${row.transport}`)
    : row.transport
}

/** "S2 · Claude user config · <file> · <account> · <status>": where it is set up. */
function sourceText(row: DiscoveryRow): string {
  const parts = [`${row.source} · ${t(`mcp.discovery.source.${row.source}`)}`]
  if (row.app) parts.push(te(`configImport.adapter.${row.app}`) ? t(`configImport.adapter.${row.app}`) : row.app)
  if (row.source !== 'S1') parts.push(row.originLabel)
  if (row.profile && row.source !== 'S1') parts.push(profileName(row.profile))
  if (row.engineStatus) parts.push(row.engineStatus)
  return parts.join(' · ')
}

async function importRow(row: DiscoveryRow) {
  try {
    const outcomes = await store.importRows([row.id])
    const failed = outcomes.find((o) => o.status === 'failed')
    if (failed) toast.error(failed.message ?? t('configImport.outcome.failed'))
    else toast.success(t('configImport.outcome.summary', outcomeCounts(outcomes)))
    emit('changed')
  } catch (e) {
    // Local-only: a remote desktop gets `remote_denied_local_only`.
    toast.error(errorText(e, t))
  }
}

async function syncRow(row: DiscoveryRow) {
  if (!row.serverId) return
  try {
    const server = await store.sync(row.serverId)
    toast.success(t('mcp.discovery.synced', { name: server.name }))
    emit('changed')
  } catch (e) {
    toast.error(errorText(e, t))
  }
}
</script>

<template>
  <div>
    <p
      v-for="p in split.problems"
      :key="`${p.source}|${p.label}`"
      class="flex items-start gap-1.5 border-b border-border/50 px-4 py-2 text-2xs text-destructive"
    >
      <AlertTriangle class="mt-px h-3 w-3 shrink-0" :stroke-width="2" />
      <span class="min-w-0 break-words">{{ p.source }} · {{ t('mcp.discovery.sourceProblem', { label: p.label, message: p.message }) }}</span>
    </p>

    <p v-if="!blocks.length && !split.problems.length" class="px-4 py-6 text-center text-xs text-muted-foreground">
      {{ t('mcp.discovery.empty') }}
    </p>

    <section v-for="block in blocks" :key="block.key" class="border-b border-border/50 last:border-b-0">
      <h4
        v-if="block.group || blocks.length > 1"
        class="flex flex-wrap items-center gap-1.5 px-4 pt-3 pb-1 text-2xs font-medium uppercase tracking-wide text-muted-foreground"
      >
        <span>{{ block.group ? t('mcp.discovery.connectorGroup', { profile: profileName(block.group.profile) }) : t('mcp.discovery.configGroup') }}</span>
        <template v-if="block.group">
          <Badge
            v-for="code in block.group.conflicts"
            :key="code"
            :tone="conflictTone(code)"
            size="xs"
            class="normal-case tracking-normal"
            :title="conflictTitle(code, block.group, null)"
          >{{ t(`mcp.discovery.conflict.${code}`) }}</Badge>
        </template>
      </h4>
      <template v-if="block.group">
        <p
          v-for="p in block.group.problems"
          :key="p.message"
          class="flex items-start gap-1.5 px-4 pb-1.5 text-2xs text-destructive"
        >
          <AlertTriangle class="mt-px h-3 w-3 shrink-0" :stroke-width="2" />
          <span class="min-w-0 break-words">{{ t('mcp.discovery.probeFailed', { message: p.message }) }}</span>
        </p>
        <p v-if="!block.rows.length && !block.group.problems.length" class="px-4 pb-2.5 text-2xs text-muted-foreground">
          {{ probing ? t('mcp.discovery.probing') : t('mcp.discovery.noConnectors') }}
        </p>
      </template>

      <ul class="divide-y divide-border/50">
        <li v-for="row in block.rows" :key="row.id" class="flex items-start gap-3 px-4 py-2.5">
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-1.5">
              <p class="truncate font-mono text-xs font-medium" :title="row.name">{{ row.name }}</p>
              <Badge tone="neutral" size="xs" class="shrink-0 uppercase tracking-wide">{{ transportText(row) }}</Badge>
              <Badge :tone="STATE_TONE[row.state]" size="xs" class="shrink-0">{{ t(`mcp.discovery.state.${row.state}`) }}</Badge>
              <Badge
                v-for="code in rowConflicts(row)"
                :key="code"
                :tone="conflictTone(code)"
                size="xs"
                class="shrink-0"
                :title="conflictTitle(code, block.group, row)"
              >{{ t(`mcp.discovery.conflict.${code}`) }}</Badge>
            </div>
            <p class="mt-0.5 truncate text-2xs text-muted-foreground" :title="sourceText(row)">{{ sourceText(row) }}</p>
            <p class="mt-0.5 text-2xs" :class="row.loadedInRuns ? 'text-success' : 'text-muted-foreground/70'">
              {{ row.loadedInRuns ? t('mcp.discovery.loaded') : t('mcp.discovery.notLoaded') }}
            </p>
            <p v-if="rowAction(row) === 'sync'" class="mt-1 text-2xs text-warning">{{ t('mcp.discovery.changedNotice') }}</p>
            <p v-else-if="rowAction(row) === 'problem'" class="mt-1 break-words text-2xs text-destructive">
              {{ t('mcp.discovery.problemLine', { message: row.problem ?? '' }) }}
            </p>
            <p v-else-if="rowAction(row) === 'blocked'" class="mt-1 text-2xs text-muted-foreground">{{ t('mcp.discovery.blockedNotice') }}</p>
          </div>
          <Button
            v-if="rowAction(row) === 'import'"
            variant="outline"
            size="xs"
            class="shrink-0"
            :disabled="store.busy.has(row.id)"
            @click="importRow(row)"
          >
            <Download class="h-3 w-3" :stroke-width="2" />
            {{ store.busy.has(row.id) ? t('mcp.list.importing') : t('mcp.list.import') }}
          </Button>
          <Button
            v-else-if="rowAction(row) === 'sync' && row.serverId"
            size="xs"
            class="shrink-0"
            :disabled="store.busy.has(row.serverId)"
            @click="syncRow(row)"
          >
            <RefreshCw class="h-3 w-3" :class="{ 'animate-spin': store.busy.has(row.serverId) }" :stroke-width="2" />
            {{ store.busy.has(row.serverId) ? t('mcp.discovery.syncing') : t('mcp.discovery.sync') }}
          </Button>
          <Button
            v-if="(rowAction(row) === 'open' || rowAction(row) === 'sync') && row.serverId"
            variant="ghost"
            size="xs"
            class="shrink-0"
            :title="t('mcp.discovery.openTitle')"
            @click="emit('open', row.serverId)"
          >
            {{ t('mcp.discovery.open') }}
            <ArrowRight class="h-3 w-3" :stroke-width="2" />
          </Button>
        </li>
      </ul>
    </section>
  </div>
</template>
