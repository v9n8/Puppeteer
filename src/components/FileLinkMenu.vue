<script setup lang="ts">
// Right-click menu on a file link in a session (UX-44): Open · Open in VS Code ·
// Reveal in Finder · Copy path. A short list on purpose: a link names a file to
// look at, not one to rename or delete — that is the explorer's menu
// (FileActionsMenu). The actions are the explorer's own, from lib/fileActions.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Code, Copy, FileText, FolderOpen } from 'lucide-vue-next'
import { ContextMenu, DropdownItem, DropdownSeparator } from '@/components/ui'
import { useFileActions, type FileTarget } from '@/lib/fileActions'

const props = defineProps<{
  projectPath: string
  /** The link right-clicked: a project-relative or absolute path. Null = closed. */
  target: { path: string; line: number | null; x: number; y: number } | null
}>()

const emit = defineEmits<{
  open: [path: string, line: number | null]
  close: []
}>()

const { t } = useI18n()
const actions = useFileActions()

const file = computed<FileTarget | null>(() => {
  const path = props.target?.path
  return path ? { path, name: path.slice(path.lastIndexOf('/') + 1), isDir: false } : null
})
</script>

<template>
  <ContextMenu :open="!!target" :x="target?.x ?? 0" :y="target?.y ?? 0" @close="emit('close')">
    <template v-if="target && file">
      <DropdownItem @click="emit('open', target.path, target.line)">
        <FileText class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        {{ t('run.open') }}
      </DropdownItem>
      <DropdownItem @click="actions.openInVscode(projectPath, file)">
        <Code class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        {{ t('files.tree.openInVscode') }}
      </DropdownItem>
      <DropdownItem @click="actions.revealInFolder(projectPath, file)">
        <FolderOpen class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        {{ t('files.tree.revealInFinder') }}
      </DropdownItem>
      <DropdownSeparator />
      <DropdownItem @click="actions.copyAbsPath(projectPath, file)">
        <Copy class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        {{ t('files.tree.copyPath') }}
      </DropdownItem>
    </template>
  </ContextMenu>
</template>
