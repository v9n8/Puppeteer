<script setup lang="ts">
// Settings → "Memory": edit the global instruction files the CLIs read on their
// own — Claude's `~/.claude/CLAUDE.md` and Codex's `~/.codex/AGENTS.md`
// (commands/memory.rs). A project's CLAUDE.md / AGENTS.md is in Library › By
// project (UX-55/D12). A missing file opens as an empty editor; a save creates
// it. A save is refused when the file changed on disk since it was opened.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Brain, Link2, RotateCw } from 'lucide-vue-next'
import { Banner, Button, Card, InfoTip, SegmentedControl } from '@/components/ui'
import SkillEditor from '@/components/SkillEditor.vue'
import SettingsRow from './SettingsRow.vue'
import { invoke } from '@/lib/tauri'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { formatSize } from '@/lib/ipc/nativeCore'
import {
  CMD_MEMORY_READ, CMD_MEMORY_WRITE, GLOBAL_MEMORY_KINDS, type GlobalMemoryKind, type MemoryFile,
  estimateTokens, isMemoryConflict, utf8Bytes,
} from '@/lib/memoryFiles'

const { t } = useI18n()
const { toast } = useToast()
const { confirm } = useConfirm()

const kind = ref<GlobalMemoryKind>('user')
const target = computed(() => ({ kind: kind.value }))

const file = ref<MemoryFile | null>(null)
const content = ref('')
const loading = ref(false)
const saving = ref(false)
const loadError = ref<string | null>(null)
const conflict = ref(false)

const kindOptions = computed(() => GLOBAL_MEMORY_KINDS.map((k) => ({ value: k, label: t(`memory.kind.${k}`) })))
const dirty = computed(() => !!file.value && content.value !== file.value.content)
const size = computed(() => formatSize(utf8Bytes(content.value)))
const tokens = computed(() => estimateTokens(content.value))

async function load() {
  conflict.value = false
  loadError.value = null
  loading.value = true
  try {
    const read = await invoke<MemoryFile>(CMD_MEMORY_READ, { target: target.value })
    file.value = read
    content.value = read.content
  } catch (e) {
    file.value = null
    content.value = ''
    loadError.value = String(e)
  } finally {
    loading.value = false
  }
}

/** Leaving unsaved edits takes a confirm. */
async function mayDiscard(): Promise<boolean> {
  if (!dirty.value) return true
  return confirm({
    title: t('memory.discardTitle'),
    message: t('memory.discardMessage'),
    confirmLabel: t('memory.discard'),
  })
}

// Settings asks before closing over unsaved edits.
defineExpose({ mayDiscard })

async function selectKind(next: GlobalMemoryKind) {
  if (next === kind.value || !(await mayDiscard())) return
  kind.value = next
  await load()
}

async function reload() {
  if (!(await mayDiscard())) return
  await load()
}

async function write(expectedHash: string | null) {
  saving.value = true
  try {
    const saved = await invoke<MemoryFile>(CMD_MEMORY_WRITE, { target: target.value, content: content.value, expectedHash })
    file.value = saved
    content.value = saved.content
    conflict.value = false
    toast.success(t('memory.saved'))
  } catch (e) {
    if (isMemoryConflict(e)) conflict.value = true
    else toast.error(String(e))
  } finally {
    saving.value = false
  }
}

function save() {
  if (file.value) void write(file.value.hash)
}

/** Keep this buffer over what changed on disk: save against the file as it is now. */
async function overwrite() {
  try {
    const current = await invoke<MemoryFile>(CMD_MEMORY_READ, { target: target.value })
    await write(current.hash)
  } catch (e) {
    toast.error(String(e))
  }
}

onMounted(load)
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Brain class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('memory.title') }}</span>
      <InfoTip :text="t('memory.hint')" />
    </template>

    <SettingsRow :label="t('memory.file')">
      <SegmentedControl :model-value="kind" :options="kindOptions" @update:model-value="selectKind" />
    </SettingsRow>

    <Banner v-if="loadError" tone="destructive">{{ loadError }}</Banner>

    <template v-else-if="file">
      <!-- Path, link target, size -->
      <div class="flex items-start justify-between gap-2">
        <div class="min-w-0 space-y-0.5">
          <div class="truncate font-mono text-2xs text-foreground" :title="file.path">{{ file.path }}</div>
          <div v-if="file.link_target" class="flex items-center gap-1 text-2xs text-muted-foreground" :title="file.link_target">
            <Link2 class="h-3 w-3 shrink-0" :stroke-width="1.75" />
            <span class="truncate">{{ t('memory.linkTo', { path: file.link_target }) }}</span>
          </div>
          <div class="text-2xs tabular-nums text-muted-foreground">
            {{ t('memory.size', { size, tokens: tokens.toLocaleString() }) }}
          </div>
          <div v-if="!file.exists" class="text-2xs text-muted-foreground">{{ t('memory.missing') }}</div>
        </div>
        <Button variant="ghost" size="icon-sm" :title="t('memory.reload')" :aria-label="t('memory.reload')" :disabled="loading" @click="reload">
          <RotateCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" :stroke-width="1.75" />
        </Button>
      </div>

      <Banner v-if="file.managed_blocks.length" tone="info">
        {{ t('memory.managedBlocks', { names: file.managed_blocks.join(', '), count: file.managed_blocks.length }, file.managed_blocks.length) }}
      </Banner>

      <Banner v-if="conflict" tone="warning" live>
        {{ t('memory.conflict') }}
        <template #actions>
          <Button variant="outline" size="xs" @click="load">{{ t('memory.reloadDiscard') }}</Button>
          <Button variant="destructive" size="xs" :disabled="saving" @click="overwrite">{{ t('memory.overwrite') }}</Button>
        </template>
      </Banner>

      <div class="h-[420px] overflow-hidden rounded-md border border-border/70">
        <SkillEditor v-model="content" :is-dark="true" />
      </div>
      <div class="flex items-center justify-end gap-2">
        <span v-if="dirty" class="text-2xs text-muted-foreground">{{ t('memory.unsaved') }}</span>
        <Button size="sm" :disabled="saving || (file.exists && !dirty)" @click="save">
          {{ file.exists ? t('common.save') : t('memory.create') }}
        </Button>
      </div>
    </template>
  </Card>
</template>
