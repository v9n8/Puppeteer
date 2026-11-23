<script setup lang="ts">
// "Clean now" (Stats › Storage): the preview of what a cleanup would remove,
// per kind, and the cleanup itself once confirmed. Cancel deletes nothing.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Loader2 } from 'lucide-vue-next'
import { Button, Modal, Banner } from '@/components/ui'
import { formatSize } from '@/lib/ipc/nativeCore'
import { errorText } from '@/lib/featureGate'
import { ESTIMATED, previewRows, type CleanupOutcome, type CleanupPreview } from '@/lib/storageCleanup'
import { runStorageCleanup } from '@/stores/stats'

const props = defineProps<{
  open: boolean
  preview: CleanupPreview | null
  loading: boolean
}>()
const emit = defineEmits<{ close: []; cleaned: [outcome: CleanupOutcome] }>()

const { t, te } = useI18n()
const running = ref(false)
const error = ref<string | null>(null)

const summary = computed(() => previewRows(props.preview?.categories ?? []))
const toClean = computed(() => summary.value.rows.filter((c) => c.items > 0))
const worktrees = computed(() => props.preview?.categories.find((c) => c.id === 'worktrees'))
const hasEstimate = computed(() => toClean.value.some((c) => ESTIMATED.has(c.id)))

function label(id: string): string {
  const key = `storageCleanup.category.${id}.label`
  return te(key) ? t(key) : id
}

function close() {
  if (running.value) return
  error.value = null
  emit('close')
}

async function clean() {
  running.value = true
  error.value = null
  try {
    const outcome = await runStorageCleanup()
    emit('cleaned', outcome)
  } catch (e) {
    error.value = String(e) === 'busy' ? t('storageCleanup.busy') : t('storageCleanup.failed', { error: errorText(e, t) })
  } finally {
    running.value = false
  }
}
</script>

<template>
  <Modal :open="open" size="md" :title="t('storageCleanup.dialog.title')" :closable="!running" scroll-body @close="close">
    <div class="space-y-3 px-4 py-3 text-xs">
      <p v-if="loading && !preview" class="flex items-center gap-1.5 text-muted-foreground">
        <Loader2 class="h-3.5 w-3.5 animate-spin" /> {{ t('storageCleanup.dialog.loading') }}
      </p>
      <template v-else-if="preview">
        <p class="text-muted-foreground">
          {{ t('storageCleanup.dialog.intro', { days: preview.effective_days }) }}
          <template v-if="preview.days === 0">{{ t('storageCleanup.dialog.offNote', { days: preview.effective_days }) }}</template>
        </p>
        <p v-if="!toClean.length" class="py-2 text-muted-foreground">{{ t('storageCleanup.dialog.empty') }}</p>
        <table v-else class="w-full">
          <thead>
            <tr class="text-left text-muted-foreground">
              <th class="pb-1.5 font-normal">{{ t('storageCleanup.dialog.kind') }}</th>
              <th class="pb-1.5 text-right font-normal">{{ t('storageCleanup.dialog.files') }}</th>
              <th class="pb-1.5 text-right font-normal">{{ t('storageCleanup.dialog.size') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="c in toClean" :key="c.id" class="border-t border-border/40">
              <td class="py-1.5">{{ label(c.id) }}</td>
              <td class="py-1.5 text-right tabular-nums text-muted-foreground">{{ c.files.toLocaleString() }}</td>
              <td class="py-1.5 text-right tabular-nums">
                {{ ESTIMATED.has(c.id) ? t('storageCleanup.about', { size: formatSize(c.bytes) }) : formatSize(c.bytes) }}
              </td>
            </tr>
            <tr class="border-t border-border font-medium">
              <td class="py-1.5">{{ t('storageCleanup.dialog.total') }}</td>
              <td class="py-1.5 text-right tabular-nums">{{ summary.files.toLocaleString() }}</td>
              <td class="py-1.5 text-right tabular-nums">{{ formatSize(summary.bytes) }}</td>
            </tr>
          </tbody>
        </table>
        <p v-if="hasEstimate" class="text-2xs text-muted-foreground">{{ t('storageCleanup.dialog.estimate') }}</p>
        <p v-if="worktrees && worktrees.kept_items > 0" class="text-2xs text-muted-foreground">
          {{ t('storageCleanup.dialog.keptWorktrees', { n: worktrees.kept_items, size: formatSize(worktrees.kept_bytes) }) }}
        </p>
      </template>
      <Banner v-if="error || preview?.busy" tone="warning">{{ error ?? t('storageCleanup.busy') }}</Banner>
    </div>
    <template #footer>
      <Button variant="ghost" :disabled="running" @click="close">{{ t('common.cancel') }}</Button>
      <Button variant="destructive" :disabled="running || loading || !toClean.length || preview?.busy" @click="clean">
        <Loader2 v-if="running" class="h-3.5 w-3.5 animate-spin" />
        {{ running ? t('storageCleanup.dialog.running') : t('storageCleanup.dialog.confirm', { size: formatSize(summary.bytes) }) }}
      </Button>
    </template>
  </Modal>
</template>
