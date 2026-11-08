<script setup lang="ts">
// Schedules (`orchestration.schedules`): prompts sent to a new session of a project
// on a timer, only while the app is open (src-tauri/src/schedules). A schedule is
// created off; an agent's proposal (MCP `schedule_propose`) must be opened and saved
// before it can be switched on. Switching on is local-only, and a save from a remote
// desktop switches the schedule off, so the toggle is read-only there.
import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Pencil, Plus, RotateCw, Trash2 } from 'lucide-vue-next'
import { AppSelect, Badge, Banner, Button, Input, Modal, PageHeader, Textarea, ToggleSwitch } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import { isRemoteMode } from '@/lib/ipc/remote'
import { useConfirm } from '@/composables/useConfirm'
import { useProjectsStore } from '@/stores/projects'
import {
  CMD_DELETE_SCHEDULE,
  CMD_LIST_SCHEDULES,
  CMD_SAVE_SCHEDULE,
  CMD_SET_SCHEDULE_ENABLED,
  EVT_SCHEDULES_CHANGED,
  PERMISSION_MODES,
  TIMING_KINDS,
  emptyForm,
  formError,
  formFromSchedule,
  inputFromForm,
  timingSummary,
  toggleWeekday,
  type Schedule,
  type ScheduleForm,
} from '@/lib/schedules'

defineOptions({ name: 'SchedulesView' })

const { t, locale } = useI18n()
const router = useRouter()
const projects = useProjectsStore()
const { confirm } = useConfirm()
const remote = isRemoteMode()

const schedules = shallowRef<Schedule[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
/** Ids whose switch is being written. */
const toggling = ref(new Set<string>())

const form = ref<ScheduleForm | null>(null)
const saving = ref(false)
const formErr = ref<string | null>(null)

const projectName = (id: string) => projects.projects.find((p) => p.id === id)?.name ?? id
const projectOptions = computed(() => projects.projects.map((p) => ({ value: p.id, label: p.name })))
const engineOptions = computed(() => [
  { value: '', label: t('schedules.engineDefault') },
  { value: 'claude', label: 'Claude' },
  { value: 'codex', label: 'Codex' },
])
const modeOptions = computed(() => PERMISSION_MODES.map((m) => ({ value: m, label: t(`schedules.mode.${m}`) })))
const kindOptions = computed(() => TIMING_KINDS.map((k) => ({ value: k, label: t(`schedules.kind.${k}`) })))
const WEEK = [1, 2, 3, 4, 5, 6, 0]
const invalidField = computed(() => (form.value ? formError(form.value) : null))

async function load() {
  loading.value = true
  try {
    schedules.value = await invoke<Schedule[]>(CMD_LIST_SCHEDULES)
    error.value = null
  } catch (e) {
    error.value = errorText(e, t)
  } finally {
    loading.value = false
  }
}

let unlisten: UnlistenFn | null = null
let disposed = false
onMounted(async () => {
  if (projects.projects.length === 0) projects.fetchProjects().catch(() => {})
  void load()
  const un = await listen(EVT_SCHEDULES_CHANGED, () => void load())
  if (disposed) un()
  else unlisten = un
})
onBeforeUnmount(() => {
  disposed = true
  unlisten?.()
})

function dayName(day: number): string {
  // 2026-11-08 is a Sunday: day 0.
  return new Date(2026, 10, 8 + day).toLocaleDateString(locale.value, { weekday: 'short' })
}

function timingText(s: Schedule): string {
  const { key, params } = timingSummary(s.timing)
  const days = Array.isArray(params.days) ? (params.days as number[]).map(dayName).join(', ') : undefined
  return t(`schedules.timing.${key}`, { ...params, days })
}

function when(iso: string | null): string {
  return iso ? new Date(iso).toLocaleString(locale.value, { dateStyle: 'medium', timeStyle: 'short' }) : '—'
}

function lastText(s: Schedule): string | null {
  if (!s.last_status) return null
  const base = t(`schedules.last.${s.last_status}`, { at: when(s.last_run_at) })
  return s.last_message ? `${base} — ${s.last_message}` : base
}

function openRun(s: Schedule) {
  if (s.last_run_id) void router.push(`/projects/${s.project_id}/run/${s.last_run_id}`)
}

/** Switching off is always allowed; switching on needs a reviewed, valid schedule. */
const canToggle = (s: Schedule) => !remote && (s.enabled || (!s.proposed && s.timing !== null))

async function toggle(s: Schedule) {
  if (!canToggle(s) || toggling.value.has(s.id)) return
  toggling.value = new Set(toggling.value).add(s.id)
  try {
    await invoke(CMD_SET_SCHEDULE_ENABLED, { id: s.id, enabled: !s.enabled })
    await load()
  } catch (e) {
    error.value = errorText(e, t)
  } finally {
    const next = new Set(toggling.value)
    next.delete(s.id)
    toggling.value = next
  }
}

function openNew() {
  formErr.value = null
  form.value = emptyForm(projects.projects[0]?.id ?? '')
}

function openEdit(s: Schedule) {
  formErr.value = null
  form.value = formFromSchedule(s)
}

async function save() {
  const f = form.value
  if (!f || invalidField.value || saving.value) return
  saving.value = true
  formErr.value = null
  try {
    await invoke(CMD_SAVE_SCHEDULE, { input: inputFromForm(f) })
    form.value = null
    await load()
  } catch (e) {
    formErr.value = errorText(e, t)
  } finally {
    saving.value = false
  }
}

async function remove(s: Schedule) {
  if (!(await confirm({ title: t('schedules.deleteTitle'), message: t('schedules.deleteMessage', { name: s.name }) }))) return
  try {
    await invoke(CMD_DELETE_SCHEDULE, { id: s.id })
    await load()
  } catch (e) {
    error.value = errorText(e, t)
  }
}
</script>

<template>
  <div class="flex flex-col h-full">
    <PageHeader :title="t('schedules.title')" :count="schedules.length || null">
      <template #actions>
        <Button variant="ghost" size="xs" :disabled="loading" @click="load">
          <RotateCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" :stroke-width="2" />
          {{ t('schedules.refresh') }}
        </Button>
        <Button size="xs" :disabled="projects.projects.length === 0" @click="openNew">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('schedules.new') }}
        </Button>
      </template>
    </PageHeader>

    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-4xl space-y-4">
        <p class="text-xs text-muted-foreground">{{ t('schedules.hint') }}</p>
        <Banner v-if="remote" tone="info">{{ t('schedules.remote') }}</Banner>
        <Banner v-if="error" tone="destructive">{{ error }}</Banner>

        <p v-if="!loading && !error && schedules.length === 0" class="text-xs text-muted-foreground">
          {{ t('schedules.empty') }}
        </p>

        <div v-if="schedules.length > 0" class="divide-y divide-border/40 rounded-lg border border-border bg-card">
          <div v-for="s in schedules" :key="s.id" class="flex items-start gap-3 px-3 py-2.5 text-xs">
            <ToggleSwitch
              class="mt-0.5"
              :checked="s.enabled"
              :disabled="!canToggle(s)"
              :loading="toggling.has(s.id)"
              :label="t('schedules.enabled')"
              @toggle="toggle(s)"
            />
            <div class="min-w-0 flex-1 space-y-0.5">
              <div class="flex flex-wrap items-center gap-2">
                <span class="font-semibold">{{ s.name }}</span>
                <Badge v-if="s.proposed" tone="warning" size="xs">{{ t('schedules.proposed') }}</Badge>
                <Badge size="xs">{{ t(`schedules.mode.${s.permission_mode}`) }}</Badge>
                <span class="text-2xs text-muted-foreground">{{ projectName(s.project_id) }}</span>
              </div>
              <p class="text-2xs text-muted-foreground">
                {{ timingText(s) }}<template v-if="s.enabled"> · {{ t('schedules.next', { at: when(s.next_run_at) }) }}</template>
              </p>
              <p class="line-clamp-2 text-2xs text-muted-foreground/80">{{ s.prompt }}</p>
              <p v-if="s.proposed" class="text-2xs text-warning">{{ t('schedules.proposedHint') }}</p>
              <p v-if="lastText(s)" class="text-2xs" :class="s.last_status === 'error' ? 'text-destructive' : 'text-muted-foreground'">
                {{ lastText(s) }}
                <button v-if="s.last_run_id" type="button" class="ml-1 underline hover:text-foreground" @click="openRun(s)">
                  {{ t('schedules.openSession') }}
                </button>
              </p>
            </div>
            <Button variant="ghost" size="icon-sm" :title="t('schedules.edit')" :aria-label="t('schedules.edit')" @click="openEdit(s)">
              <Pencil class="h-3.5 w-3.5" :stroke-width="2" />
            </Button>
            <Button variant="destructive-ghost" size="icon-sm" :title="t('schedules.delete')" :aria-label="t('schedules.delete')" @click="remove(s)">
              <Trash2 class="h-3.5 w-3.5" :stroke-width="2" />
            </Button>
          </div>
        </div>
      </div>
    </div>

    <Modal
      :open="form !== null"
      :title="form?.id ? t('schedules.editTitle') : t('schedules.newTitle')"
      size="lg"
      scroll-body
      :dismiss-on-overlay="false"
      @close="form = null"
    >
      <div v-if="form" class="space-y-3 p-4 text-xs">
        <label class="block space-y-1">
          <span class="text-muted-foreground">{{ t('schedules.field.name') }}</span>
          <Input v-model="form.name" size="sm" />
        </label>
        <div class="grid grid-cols-3 gap-2">
          <label class="block space-y-1">
            <span class="text-muted-foreground">{{ t('schedules.field.project') }}</span>
            <AppSelect v-model="form.project_id" :options="projectOptions" />
          </label>
          <label class="block space-y-1">
            <span class="text-muted-foreground">{{ t('schedules.field.engine') }}</span>
            <AppSelect v-model="form.engine" :options="engineOptions" />
          </label>
          <label class="block space-y-1">
            <span class="text-muted-foreground">{{ t('schedules.field.mode') }}</span>
            <AppSelect v-model="form.permission_mode" :options="modeOptions" />
          </label>
        </div>
        <p class="text-2xs text-muted-foreground">{{ t(`schedules.modeHint.${form.permission_mode}`) }}</p>
        <label class="block space-y-1">
          <span class="text-muted-foreground">{{ t('schedules.field.prompt') }}</span>
          <Textarea v-model="form.prompt" rows="5" />
        </label>
        <div class="flex flex-wrap items-end gap-2">
          <label class="block space-y-1">
            <span class="text-muted-foreground">{{ t('schedules.field.kind') }}</span>
            <AppSelect v-model="form.kind" :options="kindOptions" class="w-40" />
          </label>
          <label v-if="form.kind === 'interval'" class="block space-y-1">
            <span class="text-muted-foreground">{{ t('schedules.field.everyMinutes') }}</span>
            <Input
              :model-value="form.every_minutes"
              type="number"
              size="sm"
              class="w-28"
              @update:model-value="form.every_minutes = Number($event)"
            />
          </label>
          <label v-if="form.kind === 'daily' || form.kind === 'weekly'" class="block space-y-1">
            <span class="text-muted-foreground">{{ t('schedules.field.time') }}</span>
            <Input v-model="form.time" type="time" size="sm" class="w-28" />
          </label>
          <label v-if="form.kind === 'once'" class="block space-y-1">
            <span class="text-muted-foreground">{{ t('schedules.field.onceAt') }}</span>
            <Input v-model="form.once_at" type="datetime-local" size="sm" class="w-52" />
          </label>
        </div>
        <div v-if="form.kind === 'weekly'" class="flex flex-wrap gap-1.5">
          <label v-for="d in WEEK" :key="d" class="flex items-center gap-1 rounded border border-border px-2 py-1">
            <input
              type="checkbox"
              class="accent-primary"
              :checked="form.weekdays.includes(d)"
              @change="form.weekdays = toggleWeekday(form.weekdays, d)"
            >
            {{ dayName(d) }}
          </label>
        </div>
        <p v-if="invalidField" class="text-2xs text-destructive">{{ t(`schedules.formError.${invalidField}`) }}</p>
        <Banner v-if="formErr" tone="destructive">{{ formErr }}</Banner>
        <p class="text-2xs text-muted-foreground">{{ remote ? t('schedules.remoteSave') : t('schedules.saveHint') }}</p>
      </div>
      <template #footer>
        <Button variant="ghost" @click="form = null">{{ t('schedules.cancel') }}</Button>
        <Button :disabled="!!invalidField || saving" @click="save">{{ t('schedules.save') }}</Button>
      </template>
    </Modal>
  </div>
</template>
