<script setup lang="ts">
// Right pane of the Library (UX-54): one file on disk, as a Preview or in the
// Editor. A save sends back the hash the file was read with, so a change made
// elsewhere since (Claude Code, an editor, git) is never overwritten silently.
// The host keys this pane by the item's path and asks `dirty` before leaving.
// A project's memory file (`memory`, UX-55) goes through memory_* instead: it
// may not exist yet, so it opens as an empty editor and a save creates it; it
// is neither deleted nor duplicated here.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { CopyPlus, Eye, Link2, Pencil, Trash2 } from 'lucide-vue-next'
import { Badge, Banner, Button, SegmentedControl } from '@/components/ui'
import SkillEditor from '@/components/SkillEditor.vue'
import DetailHeader from '@/components/listDetail/DetailHeader.vue'
import { invoke } from '@/lib/tauri'
import { useMarkdown } from '@/lib/markdown'
import { vMermaid } from '@/lib/mermaid'
import { vCopyCode } from '@/lib/copyCode'
import { useToast } from '@/composables/useToast'
import {
  CMD_LIBRARY_READ, CMD_LIBRARY_WRITE, isLibraryConflict, type DiskFile, type DiskItem,
} from '@/lib/libraryDisk'
import {
  CMD_MEMORY_READ, CMD_MEMORY_WRITE, isMemoryConflict, type MemoryFile, type MemoryTarget,
} from '@/lib/memoryFiles'

const props = defineProps<{
  item: DiskItem
  /** Set for a memory row: the file it reads and writes through memory_*. */
  memory?: MemoryTarget | null
  /** Project tab only: Global has nowhere to duplicate to. */
  canDuplicate: boolean
  deleting: boolean
  duplicating: boolean
}>()

const emit = defineEmits<{
  saved: [file: DiskFile | MemoryFile]
  delete: []
  duplicate: []
}>()

const { t } = useI18n()
const { toast } = useToast()
const { renderText, loadMarkdown } = useMarkdown()

const file = ref<DiskFile | MemoryFile | null>(null)
const content = ref('')
const loading = ref(false)
const loadError = ref<string | null>(null)
const saving = ref(false)
const conflict = ref(false)

const dirty = computed(() => !!file.value && content.value !== file.value.content)
const canSave = computed(() => dirty.value && !saving.value)
const unreadable = computed(() => props.item.error ?? loadError.value)
const missing = computed(() => !!file.value && 'exists' in file.value && !file.value.exists)
const managedBlocks = computed(() => (file.value && 'managed_blocks' in file.value ? file.value.managed_blocks : []))

type Mode = 'preview' | 'edit'
const MODE_KEY = 'puppeteer.library.mode'
const mode = ref<Mode>(readMode())
const modeOptions = computed(() => [
  { value: 'preview' as const, label: t('library.preview'), icon: Eye },
  { value: 'edit' as const, label: t('library.editor'), icon: Pencil },
])

function readMode(): Mode {
  try {
    return localStorage.getItem(MODE_KEY) === 'edit' ? 'edit' : 'preview'
  } catch {
    return 'preview'
  }
}

function setMode(next: Mode) {
  mode.value = next
  try {
    localStorage.setItem(MODE_KEY, next)
  } catch {
    /* storage unavailable */
  }
}

const previewHtml = computed(() => (mode.value === 'preview' ? renderText(content.value) : ''))

function readFile(): Promise<DiskFile | MemoryFile> {
  return props.memory
    ? invoke<MemoryFile>(CMD_MEMORY_READ, { target: props.memory })
    : invoke<DiskFile>(CMD_LIBRARY_READ, { path: props.item.path })
}

async function load() {
  if (props.item.error) return
  loading.value = true
  conflict.value = false
  try {
    const read = await readFile()
    file.value = read
    content.value = read.content
    loadError.value = null
    // Nothing to preview yet: open the editor (not remembered as the preferred mode).
    if (missing.value) mode.value = 'edit'
  } catch (e) {
    file.value = null
    loadError.value = String(e)
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void loadMarkdown()
  void load()
})

// A rescan found the file changed on disk: show it, unless edits would be lost.
watch(
  () => props.item.hash,
  (now) => {
    if (now && file.value && now !== file.value.hash && !dirty.value && !saving.value) void load()
  },
)

/** `expectedHash`: null = the memory file didn't exist. */
async function write(expectedHash: string | null) {
  saving.value = true
  try {
    const saved = props.memory
      ? await invoke<MemoryFile>(CMD_MEMORY_WRITE, { target: props.memory, content: content.value, expectedHash })
      : await invoke<DiskFile>(CMD_LIBRARY_WRITE, { path: props.item.path, content: content.value, expectedHash })
    file.value = saved
    content.value = saved.content
    conflict.value = false
    toast.success(t('library.saved'))
    emit('saved', saved)
  } catch (e) {
    if (isLibraryConflict(e) || isMemoryConflict(e)) conflict.value = true
    else toast.error(String(e))
  } finally {
    saving.value = false
  }
}

function save() {
  if (canSave.value && file.value) void write(file.value.hash)
}

/** Keep this buffer over what changed on disk: save against the file as it is now. */
async function overwrite() {
  try {
    const current = await readFile()
    await write(current.hash)
  } catch (e) {
    toast.error(String(e))
  }
}

function onKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
    e.preventDefault()
    save()
  }
}

defineExpose({ dirty })
</script>

<template>
  <section class="flex min-w-0 flex-1 flex-col" @keydown="onKeydown">
    <DetailHeader :title="item.name">
      <template #meta>
        <span class="font-mono" :title="item.path">{{ item.path }}</span>
      </template>
      <template #actions>
        <Button size="sm" :disabled="!canSave" :title="t('library.saveHint')" @click="save">
          {{ saving ? t('library.saving') : t('common.save') }}
        </Button>
        <Button
          v-if="canDuplicate && !memory"
          variant="outline"
          size="sm"
          :disabled="duplicating || dirty || !!item.error"
          :title="dirty ? t('library.duplicateSaveFirst') : t('library.duplicateHint')"
          @click="emit('duplicate')"
        >
          <CopyPlus class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ t('library.duplicate') }}
        </Button>
        <Button
          v-if="!memory"
          variant="destructive-ghost"
          size="icon"
          :disabled="deleting || !!item.error"
          :title="t('common.delete')"
          :aria-label="t('common.delete')"
          @click="emit('delete')"
        >
          <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
      </template>
    </DetailHeader>

    <div class="flex shrink-0 items-center gap-2 border-b border-border/60 bg-muted/30 px-3 py-1">
      <div class="flex min-w-0 flex-1 items-center gap-1.5">
        <Badge size="xs" class="shrink-0">{{ t(`library.engine.${item.engine}`) }}</Badge>
        <Badge
          v-if="item.status !== 'file'"
          size="xs"
          :tone="item.status === 'puppeteer_link' ? 'warning' : 'neutral'"
          class="shrink-0"
          :title="t(`library.statusHint.${item.status}`)"
        >
          {{ t(`library.status.${item.status}`) }}
        </Badge>
        <span
          v-if="item.link_target"
          class="flex min-w-0 items-center gap-1 text-2xs text-muted-foreground"
          :title="item.link_target"
        >
          <Link2 class="h-3 w-3 shrink-0" :stroke-width="1.75" />
          <span class="truncate font-mono">{{ item.link_target }}</span>
        </span>
      </div>
      <span v-if="dirty" class="flex shrink-0 items-center gap-1 text-2xs text-muted-foreground">
        <span class="h-1.5 w-1.5 rounded-full bg-primary" />
        {{ t('library.unsaved') }}
      </span>
      <SegmentedControl
        v-if="!unreadable"
        :model-value="mode"
        :options="modeOptions"
        :stretch="false"
        @update:model-value="setMode"
      />
    </div>

    <Banner v-if="unreadable" tone="destructive" class="m-4">{{ unreadable }}</Banner>

    <template v-else>
      <Banner v-if="item.status === 'puppeteer_link'" tone="warning" class="shrink-0 rounded-none border-x-0 border-t-0 px-3 text-xs">
        {{ t('library.puppeteerLinkWarning') }}
      </Banner>
      <Banner v-if="missing" tone="info" class="shrink-0 rounded-none border-x-0 border-t-0 px-3 text-xs">
        {{ t('library.memoryMissing') }}
      </Banner>
      <Banner v-if="managedBlocks.length" tone="info" class="shrink-0 rounded-none border-x-0 border-t-0 px-3 text-xs">
        {{ t('memory.managedBlocks', { names: managedBlocks.join(', '), count: managedBlocks.length }, managedBlocks.length) }}
      </Banner>
      <Banner v-if="conflict" tone="warning" live class="shrink-0 rounded-none border-x-0 border-t-0 px-3 text-xs">
        {{ t('library.conflict') }}
        <template #actions>
          <Button variant="outline" size="xs" @click="load">{{ t('library.reloadDiscard') }}</Button>
          <Button variant="destructive" size="xs" :disabled="saving" @click="overwrite">{{ t('library.overwrite') }}</Button>
        </template>
      </Banner>

      <div v-if="loading && !file" class="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        {{ t('common.loading') }}
      </div>
      <template v-else-if="file">
        <div v-show="mode === 'edit'" class="flex min-h-0 flex-1 flex-col overflow-hidden">
          <SkillEditor v-model="content" :is-dark="true" class="flex-1" />
        </div>
        <div v-if="mode === 'preview'" class="min-h-0 flex-1 overflow-auto p-5">
          <div v-mermaid v-copy-code class="markdown-output" v-html="previewHtml" />
        </div>
      </template>
    </template>
  </section>
</template>
