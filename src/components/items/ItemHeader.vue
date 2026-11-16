<script setup lang="ts">
// Header of the Notes window's detail pane — the sibling of SlackThreadHeader,
// same layout and controls:
//
//   title (click → inline rename) / updated · N characters
//   right [Sessions (n) ▾] | ✎ Edit · ⧉ Copy ID (not in ⋯) ·
//         ⋯ (New session from this note · Make secret · Delete)
//
// The project is edited in the body (ItemFields). A todo's header is
// components/tasks/TaskDetailHeader.vue.
//
// While editing, the actions give way to Cancel · Save (⌘/Ctrl+Enter / Esc do
// the same inside the fields). Narrow pane: the title gives way first, then the
// project name truncates.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Copy, LockKeyhole, Pencil } from 'lucide-vue-next'
import { Button, DropdownItem } from '@/components/ui'
import InlineRenameInput from '@/components/InlineRenameInput.vue'
import LinkedSessionsMenu from '@/components/sessions/LinkedSessionsMenu.vue'
import DetailActionsMenu from '@/components/sessions/DetailActionsMenu.vue'
import { firstLine, relativeAge } from '@/lib/itemText'
import { formatFullDate } from '@/lib/slackThreadUtils'
import type { SessionLink } from '@/lib/sessionLinks'
import type { Note } from '@/stores/notes'

const props = defineProps<{
  item: Note
  /** Sessions linked to the item. */
  links: SessionLink[]
  renaming: boolean
  editing: boolean
  canSave: boolean
  saving: boolean
  /** Copy ID just fired: its icon becomes a tick. */
  copiedId: boolean
  /** A note can move into the secret notes vault (local windows only). */
  canMakeSecret?: boolean
}>()

const emit = defineEmits<{
  startRename: []
  commitRename: [title: string]
  cancelRename: []
  edit: []
  cancelEdit: []
  save: []
  openRun: [link: SessionLink]
  unlinkRun: [link: SessionLink]
  linkRun: [runId: string]
  newSession: []
  copyId: []
  makeSecret: []
  delete: []
}>()

const { t } = useI18n()

const heading = computed(
  () => props.item.title.trim() || firstLine(props.item.content) || t('item.untitled'),
)

/** updated · N characters */
const meta = computed(() => {
  const chars = [...props.item.content].length
  return [
    t('item.metaUpdated', { age: relativeAge(props.item.updated_at) }),
    t('item.metaChars', { n: chars }, chars),
  ].join(' · ')
})

const metaTitle = computed(() => {
  const lines = [t('item.createdAt', { date: formatFullDate(props.item.created_at) })]
  if (props.item.updated_at !== props.item.created_at) {
    lines.push(t('item.updatedAt', { date: formatFullDate(props.item.updated_at) }))
  }
  return lines.join('\n')
})
</script>

<template>
  <div class="flex shrink-0 items-center gap-3 border-b border-border/60 px-5 py-2.5">
    <div class="min-w-[6rem] flex-1">
      <div class="flex min-w-0 items-center gap-1.5">
        <InlineRenameInput
          v-if="renaming"
          class="w-full text-base font-bold leading-snug"
          :value="item.title"
          :placeholder="t('item.titlePlaceholder')"
          @commit="emit('commitRename', $event)"
          @cancel="emit('cancelRename')"
        />
        <h1
          v-else
          class="min-w-0 cursor-text truncate rounded px-0.5 -mx-0.5 text-base font-bold leading-snug transition-colors hover:bg-accent/50"
          :title="`${heading}\n${t('item.renameHint')}`"
          @click="emit('startRename')"
        >
          {{ heading }}
        </h1>
      </div>
      <p class="mt-0.5 truncate text-xs text-muted-foreground" :title="metaTitle">
        {{ meta }}
      </p>
    </div>

    <div class="flex min-w-0 items-center gap-1.5">
      <!-- The project (edited in ItemFields, in the body) decides what can be linked. -->
      <LinkedSessionsMenu
        class="shrink-0"
        :project-id="item.project_id"
        :links="links"
        @open-run="emit('openRun', $event)"
        @unlink-run="emit('unlinkRun', $event)"
        @link="emit('linkRun', $event)"
      />
      <div class="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden="true" />

      <template v-if="editing">
        <Button variant="outline" size="sm" class="shrink-0" @click="emit('cancelEdit')">
          {{ t('common.cancel') }}
        </Button>
        <Button
          variant="primary"
          size="sm"
          class="shrink-0"
          :disabled="!canSave || saving"
          :title="'⌘/Ctrl+Enter'"
          @click="emit('save')"
        >
          {{ t('common.save') }}
        </Button>
      </template>
      <template v-else>
        <Button
          variant="outline"
          size="icon"
          class="shrink-0"
          :title="t('item.editTitle')"
          :aria-label="t('item.edit')"
          @click="emit('edit')"
        >
          <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button
          variant="outline"
          size="icon"
          class="shrink-0"
          :title="t('item.copyId')"
          :aria-label="t('item.copyId')"
          @click="emit('copyId')"
        >
          <component :is="copiedId ? Check : Copy" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <DetailActionsMenu
          :new-session-label="t('item.newSessionNote')"
          :new-session-hint="item.project_id ? t('item.newSessionHint') : t('item.needProject')"
          :can-new-session="!!item.project_id"
          :copied-id="copiedId"
          :show-copy-id="false"
          @new-session="emit('newSession')"
          @copy-id="emit('copyId')"
          @delete="emit('delete')"
        >
          <DropdownItem v-if="canMakeSecret" @click="emit('makeSecret')">
            <LockKeyhole class="h-3.5 w-3.5" :stroke-width="1.75" />
            {{ t('secretNotes.makeSecret.action') }}
          </DropdownItem>
        </DetailActionsMenu>
      </template>
    </div>
  </div>
</template>
