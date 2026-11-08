<script setup lang="ts">
// Confirm "Rewind to here" (backend: runs::rewind). Nothing happens until the
// user has seen what the files and the conversation go back to, and every file
// the restore overwrites or deletes — those edited after the session stopped
// first and flagged, since their edits are lost. The rewind is refused if the
// files change after this list was made.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle } from 'lucide-vue-next'
import { Button, Modal } from '@/components/ui'
import { useRunsStore, type RewindOutcome, type RewindPreview } from '@/stores/runs'

const props = defineProps<{
  runId: string | null
  /** The snapshot turn to go back to; null = closed. */
  turn: number | null
}>()
const emit = defineEmits<{ close: []; done: [outcome: RewindOutcome] }>()

const { t } = useI18n()
const runs = useRunsStore()

const preview = ref<RewindPreview | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)

const KNOWN_ERRORS = [
  'run_active',
  'folder_busy',
  'files_changed',
  'snapshot_damaged',
  'snapshot_missing',
  'snapshot_unavailable',
  'snapshot_not_found',
  'workspace_missing',
  'too_many_files',
]

function errorText(e: unknown): string {
  const raw = String(e)
  const code = raw.split(':')[0]
  return KNOWN_ERRORS.includes(code) ? t(`run.rewind.errors.${code}`) : raw
}

async function load() {
  const { runId, turn } = props
  preview.value = null
  if (!runId || turn === null) return
  try {
    const p = await runs.previewRewind(runId, turn)
    if (props.runId === runId && props.turn === turn) preview.value = p
  } catch (e) {
    if (props.runId === runId && props.turn === turn) error.value = errorText(e)
  }
}

watch(
  () => [props.runId, props.turn] as const,
  () => {
    error.value = null
    void load()
  },
  { immediate: true },
)

const changes = computed(() =>
  [...(preview.value?.files.changes ?? [])].sort((a, b) => Number(b.outside) - Number(a.outside)),
)
const outsideCount = computed(() => changes.value.filter((c) => c.outside).length)
const when = computed(() => {
  const at = preview.value?.created_at
  return at ? new Date(at).toLocaleString() : ''
})

async function confirm() {
  const { runId, turn } = props
  const p = preview.value
  if (!runId || turn === null || !p || busy.value) return
  busy.value = true
  error.value = null
  try {
    emit('done', await runs.rewindRun(runId, turn, p.fingerprint))
  } catch (e) {
    error.value = errorText(e)
    // The list no longer matches the folder: show the current one.
    if (String(e).startsWith('files_changed')) await load()
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Modal :open="!!runId && turn !== null" :title="t('run.rewind.title')" size="md" scroll-body @close="busy || emit('close')">
    <div class="space-y-3 px-4 py-4 text-sm">
      <p v-if="!preview && !error" class="text-muted-foreground">{{ t('run.rewind.loading') }}</p>
      <template v-if="preview">
        <p>{{ t('run.rewind.filesAt', { when }) }}</p>
        <p :class="preview.conversation ? 'text-muted-foreground' : 'text-warning'">
          {{
            preview.conversation
              ? t('run.rewind.conversation')
              : t('run.rewind.filesOnly', { reason: t(`run.rewind.reason.${preview.conversation_reason ?? 'engine'}`) })
          }}
        </p>
        <template v-if="changes.length">
          <p class="font-medium">{{ t('run.rewind.changes', { n: changes.length }, changes.length) }}</p>
          <p v-if="outsideCount" class="flex items-start gap-1.5 text-destructive">
            <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
            {{ t('run.rewind.outsideNote') }}
          </p>
          <ul class="max-h-64 overflow-auto rounded-md border border-border font-mono text-xs">
            <li v-for="c in changes" :key="c.path" class="flex items-center gap-2 px-2 py-1">
              <span class="w-16 shrink-0 text-muted-foreground">{{ t(`run.rewind.change.${c.change}`) }}</span>
              <span class="min-w-0 truncate" :class="{ 'text-destructive': c.outside }" :title="c.path">{{ c.path }}</span>
              <span v-if="c.outside" class="ml-auto shrink-0 font-sans text-destructive">{{ t('run.rewind.outside') }}</span>
            </li>
          </ul>
        </template>
        <p v-else class="text-muted-foreground">{{ t('run.rewind.noChanges') }}</p>
        <p v-if="preview.files.kept.length" class="text-xs text-muted-foreground">
          {{ t('run.rewind.kept', { files: preview.files.kept.join(', ') }) }}
        </p>
      </template>
      <p v-if="error" class="text-destructive">{{ error }}</p>
    </div>
    <template #footer>
      <Button variant="outline" :disabled="busy" @click="emit('close')">{{ t('common.cancel') }}</Button>
      <Button variant="destructive" :disabled="!preview || busy" @click="confirm">{{ t('run.rewind.confirm') }}</Button>
    </template>
  </Modal>
</template>
