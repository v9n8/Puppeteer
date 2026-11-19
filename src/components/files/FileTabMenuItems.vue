<script setup lang="ts">
// The rows of an open file's right-click menu, shared by the file tab
// (SessionTabBar) and the file name in the viewer's toolbar (FileViewer), so the
// file's own window keeps them too. Three groups: copy out, open elsewhere, and
// what to do with it here. Renaming, deleting, cutting and duplicating the file
// stay in the Files tree's own menu; copying the content is a toolbar button.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { AtSign, Code2, Columns2, ExternalLink, FolderSearch, Globe, Link, Link2 } from 'lucide-vue-next'
import { DropdownItem, DropdownSeparator } from '@/components/ui'
import { useFileActions, type FileTarget } from '@/lib/fileActions'
import { compareKind } from '@/lib/markdownOutline'
import { desktopOs } from '@/lib/terminalApps'

const props = defineProps<{
  projectPath: string
  target: FileTarget
  /** Image comparison needs the main window's compare host. */
  canCompareImage: boolean
}>()
const emit = defineEmits<{ compare: [] }>()

const { t } = useI18n()
const actions = useFileActions()

const os = desktopOs(navigator.userAgent)
const REVEAL_KEY = { macos: 'revealInFinder', windows: 'revealInExplorer', linux: 'revealInFolder' } as const

const kind = computed(() => compareKind(props.target.path))
const compareLock = computed(() => {
  if (!kind.value) return t('files.compare.lockedType')
  if (kind.value === 'image' && !props.canCompareImage) return t('files.compare.lockedWindow')
  return ''
})
</script>

<template>
  <DropdownItem @click="actions.copyAbsPath(projectPath, target)">
    <Link class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    {{ t('files.tree.copyPath') }}
  </DropdownItem>
  <DropdownItem @click="actions.copyRelPath(target)">
    <Link2 class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    {{ t('files.tree.copyRelativePath') }}
  </DropdownItem>
  <DropdownItem @click="actions.revealInFolder(projectPath, target)">
    <FolderSearch class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    {{ t(`files.tree.${REVEAL_KEY[os]}`) }}
  </DropdownItem>

  <DropdownSeparator />
  <DropdownItem @click="actions.openInDefaultApp(projectPath, target)">
    <ExternalLink class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    {{ t('files.tree.openInDefaultApp') }}
  </DropdownItem>
  <DropdownItem @click="actions.openInVscode(projectPath, target)">
    <Code2 class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    {{ t('files.tree.openInVscode') }}
  </DropdownItem>
  <DropdownItem @click="actions.openInChrome(projectPath, target)">
    <Globe class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    {{ t('files.tree.openInChrome') }}
  </DropdownItem>

  <DropdownSeparator />
  <DropdownItem @click="actions.mentionInChat(projectPath, target)">
    <AtSign class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    {{ t('files.tree.mentionInChat') }}
  </DropdownItem>
  <!-- A locked row is a disabled button (no pointer events), so the reason rides on a wrapper. -->
  <div :title="compareLock || undefined">
    <DropdownItem :disabled="!!compareLock" @click="emit('compare')">
      <Columns2 class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ t('files.tree.compareWith') }}
    </DropdownItem>
  </div>
</template>
