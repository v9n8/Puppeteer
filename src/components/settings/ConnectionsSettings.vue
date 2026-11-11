<script setup lang="ts">
// Settings → Connections: every account of every enabled connection type
// (lib/connectionTypes) in one list, filtered by type. A row's Validate runs the
// type's validate command; clicking the row opens the account in its type's own
// editor (edit, delete, sign in again…), and "Add connection" opens that
// editor's add flow. `filter` is v-model so `?section=github` deep links land
// on that type.
import { computed, onMounted, ref, watch, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, CheckCircle2, KeyRound, LogIn, Plug, Plus } from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import { Button, Card, DropdownItem, DropdownMenu, InfoTip, Modal, SegmentedControl, type SegmentedOption } from '@/components/ui'
import GithubAccountsSettings from '@/components/settings/GithubAccountsSettings.vue'
import GitlabAccountsSettings from '@/components/settings/GitlabAccountsSettings.vue'
import AwsAccountsSettings from '@/components/settings/AwsAccountsSettings.vue'
import AzureAccountsSettings from '@/components/AzureAccountsSettings.vue'
import GoogleAccountsSettings from '@/components/settings/GoogleAccountsSettings.vue'
import { useConnectionSources } from '@/components/settings/useConnectionSources'
import { useProjectsStore } from '@/stores/projects'
import {
  buildConnectionRows, checkResult, connectionType, consumerLabelKey, effectiveFilter, filterConnectionRows,
  type CheckResult, type ConnectionFilter, type ConnectionRow, type ConnectionTypeId,
} from '@/lib/connectionTypes'

const filter = defineModel<ConnectionFilter>('filter', { default: 'all' })

const { t } = useI18n()
const { sources, fetchers, types } = useConnectionSources()
const projectsStore = useProjectsStore()

// Each type's editor: its add flow (no `accountId`) and one account's
// validate / edit / delete / sign-in (with it).
const EDITORS: Record<ConnectionTypeId, Component> = {
  github: GithubAccountsSettings,
  gitlab: GitlabAccountsSettings,
  aws: AwsAccountsSettings,
  azure: AzureAccountsSettings,
  google: GoogleAccountsSettings,
}

// SettingsView fetches GitHub / GitLab / AWS before first paint; Azure and Google
// were fetched by their own sections, now by the list that shows them.
onMounted(() => {
  fetchers.azure().catch(() => {})
  fetchers.google().catch(() => {})
  if (projectsStore.projects.length === 0) projectsStore.fetchProjects().catch(() => {})
})

const rows = computed(() => buildConnectionRows(types.value, sources.value, projectsStore.projects))
const activeFilter = computed(() => effectiveFilter(filter.value, types.value))
const visibleRows = computed(() => filterConnectionRows(rows.value, activeFilter.value))
const filterOptions = computed<SegmentedOption<ConnectionFilter>[]>(() => [
  { value: 'all', label: t('settings.connections.all') },
  ...types.value.map((def) => ({ value: def.id as ConnectionTypeId, label: t(def.labelKey), icon: def.icon })),
])

// --- Validate from the row (the editor keeps its own, fuller result) ---
const rowKey = (r: { type: string; id: string }) => `${r.type}:${r.id}`
const checks = ref<Record<string, CheckResult>>({})
const checking = ref<Record<string, boolean>>({})

async function validate(row: ConnectionRow) {
  const key = rowKey(row)
  if (checking.value[key]) return
  const def = connectionType(row.type)
  checking.value[key] = true
  delete checks.value[key]
  try {
    checks.value[key] = checkResult(def, { ok: await invoke(def.validateCommand, { id: row.id }) })
  } catch (e) {
    checks.value[key] = checkResult(def, { error: e })
  } finally {
    checking.value[key] = false
  }
  // Validating stores what it learnt (username, last check, sign-in state).
  await fetchers[row.type]().catch(() => {})
}

// The row's last Validate result as shown: tone + text.
function checkLine(row: ConnectionRow): { outcome: CheckResult['outcome']; text: string } | null {
  const c = checks.value[rowKey(row)]
  if (!c) return null
  if (c.outcome === 'ok') return { outcome: 'ok', text: t('settings.connections.check.ok') }
  if (c.outcome === 'error') return { outcome: 'error', text: c.message }
  return { outcome: c.outcome, text: t(c.messageKey) }
}

function statusText(row: ConnectionRow): string {
  if (row.status === 'validated' && row.validatedAt) {
    const d = new Date(row.validatedAt)
    if (!isNaN(d.getTime())) return t('settings.connections.status.validatedAt', { time: d.toLocaleString() })
  }
  return t(`settings.connections.status.${row.status}`)
}

// --- Editor dialog: one account, or a type's add flow ---
const editor = ref<{ type: ConnectionTypeId; accountId: string | null } | null>(null)
const editorDef = computed(() => (editor.value ? connectionType(editor.value.type) : null))
const editorRow = computed(() =>
  editor.value?.accountId ? rows.value.find((r) => r.type === editor.value!.type && r.id === editor.value!.accountId) : undefined,
)
const editorTitle = computed(() => {
  if (!editor.value || !editorDef.value) return ''
  const type = t(editorDef.value.labelKey)
  return editor.value.accountId ? `${type} · ${editorRow.value?.label ?? ''}` : t('settings.connections.addTitle', { type })
})

function openAccount(row: ConnectionRow) {
  editor.value = { type: row.type, accountId: row.id }
}
function openAdd(type: ConnectionTypeId) {
  editor.value = { type, accountId: null }
}

// Close once the account is deleted, or its type is switched off meanwhile.
watch([rows, types], () => {
  const e = editor.value
  if (!e) return
  if (!types.value.some((def) => def.id === e.type)) editor.value = null
  else if (e.accountId && !editorRow.value) editor.value = null
})
</script>

<template>
  <div>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Plug class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.connections.title') }}</span>
      <InfoTip :text="t('settings.connections.intro')" />
      <DropdownMenu align="right" class="ml-auto">
        <template #trigger>
          <Button size="xs">
            <Plus class="h-3 w-3" :stroke-width="2" />
            {{ t('settings.connections.add') }}
          </Button>
        </template>
        <DropdownItem v-for="def in types" :key="def.id" @click="openAdd(def.id as ConnectionTypeId)">
          <component :is="def.icon" class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.75" />
          {{ t(def.labelKey) }}
        </DropdownItem>
      </DropdownMenu>
    </template>

    <SegmentedControl v-if="types.length > 1" :model-value="activeFilter" :options="filterOptions" @update:model-value="filter = $event" />

    <!-- Unified list -->
    <div v-if="visibleRows.length" class="space-y-2">
      <div
        v-for="row in visibleRows"
        :key="rowKey(row)"
        role="button"
        tabindex="0"
        class="border border-border rounded-md p-3 flex items-center gap-3 cursor-pointer hover:bg-accent/40 transition-colors"
        :title="t('settings.connections.openHint')"
        @click="openAccount(row)"
        @keydown.enter.self="openAccount(row)"
      >
        <component :is="connectionType(row.type).icon" class="h-4 w-4 shrink-0 text-muted-foreground" :stroke-width="1.5" />
        <div class="min-w-0 flex-1">
          <div class="text-sm font-medium truncate flex items-center gap-1.5">
            {{ row.label }}
            <span v-if="row.isDefault" class="text-2xs font-medium text-primary border border-primary/40 rounded px-1">{{ t('settings.google.default') }}</span>
          </div>
          <div class="text-2xs text-muted-foreground truncate">
            <span>{{ t(connectionType(row.type).labelKey) }}</span>
            <span v-if="row.detail"> · {{ row.detail }}</span>
            <span v-if="row.projectCount !== null"> · {{ t('settings.connections.projects', row.projectCount) }}</span>
          </div>
          <!-- Latest row Validate result, else the stored status -->
          <div
            v-if="checkLine(row)"
            class="text-2xs truncate flex items-center gap-1"
            :class="{ ok: 'text-success', error: 'text-destructive', warn: 'text-warning', needsLogin: 'text-warning' }[checkLine(row)!.outcome]"
            :title="checkLine(row)!.text"
          >
            <CheckCircle2 v-if="checkLine(row)!.outcome === 'ok'" class="h-3 w-3 shrink-0" :stroke-width="1.75" />
            <LogIn v-else-if="checkLine(row)!.outcome === 'needsLogin'" class="h-3 w-3 shrink-0" :stroke-width="1.75" />
            <AlertTriangle v-else class="h-3 w-3 shrink-0" :stroke-width="1.75" />
            <span class="truncate">{{ checkLine(row)!.text }}</span>
          </div>
          <div
            v-else
            class="text-2xs truncate"
            :class="row.status === 'needsLogin' ? 'text-warning' : 'text-muted-foreground'"
          >{{ statusText(row) }}</div>
        </div>
        <Button
          variant="outline"
          size="xs"
          class="shrink-0"
          :disabled="checking[rowKey(row)]"
          @click.stop="validate(row)"
        >
          {{ checking[rowKey(row)] ? '…' : t('settings.connections.validate') }}
        </Button>
      </div>
    </div>
    <p v-else class="text-2xs text-muted-foreground">
      {{ activeFilter === 'all'
        ? t('settings.connections.empty')
        : t('settings.connections.emptyType', { type: t(connectionType(activeFilter).labelKey) }) }}
    </p>
  </Card>

  <Modal
    :open="!!editor"
    :title="editorTitle"
    :dismiss-on-overlay="false"
    scroll-body
    @close="editor = null"
  >
    <div v-if="editor && editorDef" class="p-4 space-y-3">
      <div v-if="editor.accountId" class="text-2xs text-muted-foreground space-y-1">
        <p>
          <span class="font-medium">{{ t('settings.connections.usedBy') }}:</span>
          {{ editorDef.consumers.map((c) => t(consumerLabelKey(c))).join(' · ') }}
        </p>
        <p v-if="editorDef.secretKeys.length" class="flex items-center gap-1">
          <KeyRound class="h-3 w-3 shrink-0" :stroke-width="1.75" />
          <span>{{ t('settings.connections.keychain') }}: {{ editorDef.secretKeys.map((k) => t(k)).join(', ') }}</span>
        </p>
      </div>
      <component :is="EDITORS[editor.type]" :account-id="editor.accountId" />
    </div>
  </Modal>
  </div>
</template>
