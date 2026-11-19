<script setup lang="ts">
// The file name at the head of the viewer's toolbar, and the "Copy content"
// button beside it. Right-click the name for the file tab's menu
// (FileTabMenuItems), so the file's own window has the same rows as the tab.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ClipboardCopy } from 'lucide-vue-next'
import { Button, ContextMenu } from '@/components/ui'
import { useFileActions, type FileTarget } from '@/lib/fileActions'
import FileTabMenuItems from './FileTabMenuItems.vue'

const props = defineProps<{
  projectPath: string
  /** The loaded file's path; '' while nothing is loaded. */
  path: string
  /** The whole text is readable: not a binary, an oversized or a failed file. */
  canCopy: boolean
  /** Image comparison needs the main window's compare host. */
  canCompareImage: boolean
}>()
const emit = defineEmits<{ compare: [] }>()

const { t } = useI18n()
const actions = useFileActions()

const menu = ref<{ x: number; y: number } | null>(null)
const target = computed<FileTarget | null>(() =>
  props.path ? { path: props.path, name: props.path.split('/').pop() || props.path, isDir: false } : null,
)

function openMenu(e: MouseEvent) {
  if (target.value) menu.value = { x: e.clientX, y: e.clientY }
}
</script>

<template>
  <span class="text-sm font-mono text-foreground truncate grow basis-32" :title="path" @contextmenu.prevent="openMenu">{{ path }}</span>
  <Button
    v-if="canCopy && target"
    variant="ghost"
    size="icon-sm"
    class="shrink-0"
    :title="t('files.tree.copyContent')"
    :aria-label="t('files.tree.copyContent')"
    @click="actions.copyContent(projectPath, target)"
  >
    <ClipboardCopy class="h-3.5 w-3.5" :stroke-width="1.75" />
  </Button>
  <ContextMenu :open="!!menu" :x="menu?.x ?? 0" :y="menu?.y ?? 0" @close="menu = null">
    <FileTabMenuItems v-if="target" :project-path="projectPath" :target="target" :can-compare-image="canCompareImage" @compare="emit('compare')" />
  </ContextMenu>
</template>
