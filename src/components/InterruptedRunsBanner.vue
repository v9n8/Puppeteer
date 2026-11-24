<script setup lang="ts">
// Main window, once per launch: sessions the app cut off when it last closed
// (HD-04 marks them, HD-05 lists / dismisses / resumes them). A run may have
// stopped half-way through writing a file, so nothing here resumes on its own:
// `resume_interrupted_runs` is only called from the Resume buttons. A
// conductor's cut-off workers count on its row; the backend tells it about them
// when it resumes.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle } from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import { Button, Modal } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import {
  applyOutcomes, rowIds, toggleSelected, type InterruptedRun, type ResumeOutcome,
} from '@/lib/interruptedRuns'

const { t } = useI18n()
const { toast } = useToast()

const rows = ref<InterruptedRun[]>([])
const errors = ref<Record<string, string>>({})
const busy = ref(false)
const pickOpen = ref(false)
const picked = ref<string[]>([])

const count = computed(() => rows.value.length)

onMounted(async () => {
  try {
    rows.value = await invoke<InterruptedRun[]>('list_interrupted_runs')
  } catch (e) {
    // No banner is the safe side: the runs keep waiting for the next launch.
    console.warn('[interrupted-runs] list failed', e)
  }
})

function rowTitle(row: InterruptedRun): string {
  return row.title?.trim() || t('run.labelSession')
}

function rowMeta(row: InterruptedRun): string {
  const parts = [row.project_name, row.engine]
  if (row.worker_count > 0) parts.push(t('interruptedRuns.workers', { n: row.worker_count }, row.worker_count))
  return parts.join(' · ')
}

async function resume(ids: string[]) {
  if (!ids.length || busy.value) return
  busy.value = true
  try {
    const outcomes = await invoke<ResumeOutcome[]>('resume_interrupted_runs', { runIds: ids })
    const next = applyOutcomes({ rows: rows.value, errors: errors.value }, outcomes)
    rows.value = next.rows
    errors.value = next.errors
  } catch (e) {
    toast.error(t('interruptedRuns.failed', { error: String(e) }))
  } finally {
    busy.value = false
  }
}

function resumeAll() {
  void resume(rowIds(rows.value))
}

function openPicker() {
  picked.value = []
  pickOpen.value = true
}

function togglePick(id: string) {
  picked.value = toggleSelected(rows.value, picked.value, id)
}

async function resumePicked() {
  const ids = picked.value
  pickOpen.value = false
  await resume(ids)
}

async function dismiss() {
  if (busy.value) return
  busy.value = true
  try {
    await invoke('dismiss_interrupted_runs', { runIds: rowIds(rows.value) })
    rows.value = []
    errors.value = {}
  } catch (e) {
    toast.error(t('interruptedRuns.failed', { error: String(e) }))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div
    v-if="count > 0"
    class="fixed top-14 left-1/2 z-50 w-[min(36rem,calc(100vw-2rem))] -translate-x-1/2 rounded-lg border border-border bg-card px-3.5 py-3 text-xs shadow-md"
    role="status"
  >
    <div class="flex items-start gap-2">
      <AlertTriangle class="mt-px h-4 w-4 shrink-0 text-warning" :stroke-width="2" />
      <div class="min-w-0 flex-1">
        <p class="text-sm font-semibold text-foreground">{{ t('interruptedRuns.title', { n: count }, count) }}</p>
        <p class="mt-0.5 text-muted-foreground">{{ t('interruptedRuns.message') }}</p>
      </div>
    </div>
    <ul class="mt-2 max-h-48 space-y-1.5 overflow-y-auto pl-6">
      <li v-for="row in rows" :key="row.run_id" class="min-w-0">
        <p class="truncate font-medium text-foreground" :title="rowTitle(row)">{{ rowTitle(row) }}</p>
        <p class="truncate text-2xs text-muted-foreground">
          {{ rowMeta(row) }} · {{ t('interruptedRuns.mode', { mode: row.permission_mode }) }}
        </p>
        <p v-if="row.run_id in errors" class="text-2xs text-destructive">
          {{ errors[row.run_id] || t('interruptedRuns.resumeFailed') }}
        </p>
      </li>
    </ul>
    <div class="mt-3 flex justify-end gap-2">
      <Button size="xs" variant="ghost" :disabled="busy" @click="dismiss">{{ t('interruptedRuns.dismiss') }}</Button>
      <Button size="xs" variant="outline" :disabled="busy" @click="openPicker">{{ t('interruptedRuns.choose') }}</Button>
      <Button size="xs" :disabled="busy" @click="resumeAll">{{ t('interruptedRuns.resumeAll') }}</Button>
    </div>
  </div>

  <Modal :open="pickOpen" size="md" :title="t('interruptedRuns.pick.title')" scroll-body @close="pickOpen = false">
    <ul class="space-y-1 px-4 py-3">
      <li v-for="row in rows" :key="row.run_id">
        <label class="flex cursor-pointer items-start gap-2 rounded-md px-2 py-1.5 hover:bg-accent">
          <input
            type="checkbox"
            class="mt-0.5 accent-primary"
            :checked="picked.includes(row.run_id)"
            @change="togglePick(row.run_id)"
          >
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm text-foreground">{{ rowTitle(row) }}</span>
            <span class="block truncate text-2xs text-muted-foreground">
              {{ rowMeta(row) }} · {{ t('interruptedRuns.mode', { mode: row.permission_mode }) }}
            </span>
          </span>
        </label>
      </li>
    </ul>
    <template #footer>
      <Button variant="ghost" @click="pickOpen = false">{{ t('common.cancel') }}</Button>
      <Button :disabled="!picked.length || busy" @click="resumePicked">
        {{ t('interruptedRuns.pick.resume', { n: picked.length }) }}
      </Button>
    </template>
  </Modal>
</template>
