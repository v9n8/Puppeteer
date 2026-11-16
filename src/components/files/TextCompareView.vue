<script setup lang="ts">
// The viewer's body while "Compare with…" shows a text file: what changes from
// the file being viewed (`path`, as it is on screen) to the chosen `other`.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Columns2, X } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import DiffView from '@/components/DiffView.vue'
import { useRunsStore } from '@/stores/runs'

const props = defineProps<{
  projectPath: string
  path: string
  /** The viewed file's text; null when it could not be loaded in full. */
  content: string | null
  other: string
}>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const runsStore = useRunsStore()

const otherContent = ref<string | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

async function loadOther() {
  loading.value = true
  error.value = null
  otherContent.value = null
  const wanted = props.other
  try {
    const res = await runsStore.readProjectFile(props.projectPath, wanted)
    if (wanted !== props.other) return
    if (res.truncated) error.value = t('files.compare.tooLarge', { name: wanted })
    else otherContent.value = res.content
  } catch (e) {
    if (wanted === props.other) error.value = String(e)
  } finally {
    if (wanted === props.other) loading.value = false
  }
}

watch(() => [props.projectPath, props.other], loadOther, { immediate: true })
</script>

<template>
  <div class="flex flex-col gap-3 p-4">
    <div class="flex items-center gap-2 text-xs font-mono">
      <Columns2 class="h-3.5 w-3.5 shrink-0 text-primary" :stroke-width="1.75" />
      <span class="min-w-0 truncate text-red-500" :title="path">− {{ path }}</span>
      <span class="min-w-0 truncate text-emerald-500" :title="other">+ {{ other }}</span>
      <span class="flex-1" />
      <Button variant="outline" size="xs" class="shrink-0" @click="emit('close')">
        <X class="h-3 w-3" :stroke-width="2" />
        {{ t('files.compare.close') }}
      </Button>
    </div>
    <p v-if="content === null" class="text-sm text-foreground/60">{{ t('files.compare.sourceUnavailable') }}</p>
    <p v-else-if="loading" class="text-sm text-foreground/50">{{ t('common.loading') }}</p>
    <p v-else-if="error" class="text-sm text-destructive break-all">{{ error }}</p>
    <p v-else-if="otherContent === content" class="text-sm text-foreground/60">{{ t('files.compare.identical') }}</p>
    <DiffView v-else-if="otherContent !== null" :before="content" :after="otherContent" />
  </div>
</template>
