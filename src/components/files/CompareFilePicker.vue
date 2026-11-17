<script setup lang="ts">
// Popover of the viewer's "Compare with…": the project's files of the same kind
// as the one being viewed (images, or text files), filtered by a search box.
// The host places it; picking a row or closing it emits.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { FileText, Image as ImageIcon, Search, X } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { compareKind } from '@/lib/markdownOutline'
import { useRunsStore } from '@/stores/runs'

const props = defineProps<{
  projectPath: string
  /** The file being viewed (left out of the list). */
  path: string
  kind: 'image' | 'text'
}>()
const emit = defineEmits<{ pick: [path: string]; close: [] }>()

const { t } = useI18n()
const runsStore = useRunsStore()

const files = ref<string[]>([])
const loading = ref(true)
const error = ref<string | null>(null)
const query = ref('')
const inputEl = ref<HTMLInputElement | null>(null)

const shown = computed(() => {
  const q = query.value.trim().toLowerCase()
  return files.value.filter(f => !q || f.toLowerCase().includes(q))
})

onMounted(async () => {
  inputEl.value?.focus()
  try {
    const entries = await runsStore.listProjectFiles(props.projectPath)
    files.value = entries
      .filter(e => !e.is_dir && e.path !== props.path && compareKind(e.path) === props.kind)
      .map(e => e.path)
      .sort((a, b) => a.localeCompare(b))
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div
    class="w-[300px] max-w-[calc(100%-2rem)] rounded-md border border-border bg-popover shadow-lg shadow-black/30"
    role="dialog"
    :aria-label="kind === 'image' ? t('files.compare.pickSecond') : t('files.compare.pickSecondFile')"
    @keydown.esc.stop.prevent="emit('close')"
  >
    <div class="flex items-center gap-1 min-h-[44px] pl-3 pr-2">
      <p class="flex-1 text-sm font-semibold text-foreground">
        {{ kind === 'image' ? t('files.compare.pickSecond') : t('files.compare.pickSecondFile') }}
      </p>
      <Button variant="ghost" size="icon-sm" :title="t('common.close')" :aria-label="t('common.close')" @click="emit('close')">
        <X class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
    </div>
    <div class="px-2 pb-2">
      <label class="flex items-center gap-2 h-8 rounded-md border border-border bg-background px-2 focus-within:ring-1 focus-within:ring-ring">
        <Search class="h-3.5 w-3.5 shrink-0 text-foreground/40" :stroke-width="1.75" />
        <input
          ref="inputEl"
          v-model="query"
          type="text"
          class="flex-1 min-w-0 bg-transparent text-sm text-foreground placeholder:text-foreground/40 outline-none"
          :placeholder="kind === 'image' ? t('files.compare.searchPlaceholder') : t('files.compare.searchFiles')"
        />
      </label>
    </div>
    <div class="max-h-64 overflow-y-auto px-1 pb-1">
      <p v-if="loading" class="px-3 py-2 text-sm text-foreground/50">{{ t('common.loading') }}</p>
      <p v-else-if="error" class="px-3 py-2 text-sm text-destructive break-all">
        {{ t('files.compare.listFailed', { error }) }}
      </p>
      <p v-else-if="!shown.length" class="px-3 py-2 text-sm text-foreground/50">
        {{ kind === 'image' ? t('files.compare.noImages') : t('files.compare.noFiles') }}
      </p>
      <button
        v-for="f in shown"
        :key="f"
        type="button"
        class="flex w-full items-center gap-2 rounded px-2 min-h-[28px] text-left text-sm text-popover-foreground hover:bg-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer"
        :title="f"
        @click="emit('pick', f)"
      >
        <ImageIcon v-if="kind === 'image'" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        <FileText v-else class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        <span class="truncate">{{ f }}</span>
      </button>
    </div>
  </div>
</template>
