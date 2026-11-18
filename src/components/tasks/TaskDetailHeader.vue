<script setup lang="ts">
// Head of the task detail panel (V3-38): the Done tick, "<Project> · <status>",
// the Sessions menu, Edit (E) and ⋯ on the first row (while editing: Cancel ·
// Save, and in the dock a close button); then the title and a meta line
// "Created … · from <session> · Due today". The fields, the AI block and the
// body are the panel's other parts.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, X } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import LinkedSessionsMenu from '@/components/sessions/LinkedSessionsMenu.vue'
import DetailActionsMenu from '@/components/sessions/DetailActionsMenu.vue'
import { useSessionText } from '@/composables/useSessionText'
import { useProjectsStore } from '@/stores/projects'
import { firstLine, relativeAge } from '@/lib/itemText'
import { formatFullDate } from '@/lib/slackThreadUtils'
import { daysUntil, dueStatus, localYmd } from '@/lib/todoDue'
import type { FocusTodo } from '@/lib/focus'
import type { SessionLink } from '@/lib/sessionLinks'

const props = defineProps<{
  todo: FocusTodo
  links: SessionLink[]
  editing: boolean
  canSave: boolean
  saving: boolean
  copiedId: boolean
  /** The dock shows a close button; the list pane has none. */
  closable: boolean
}>()

const emit = defineEmits<{
  toggleDone: []
  edit: []
  cancelEdit: []
  save: []
  close: []
  openRun: [link: SessionLink]
  unlinkRun: [link: SessionLink]
  linkRun: [runId: string]
  newSession: []
  copyId: []
  delete: []
}>()

const { t } = useI18n()
const { runLabel } = useSessionText()
const projects = useProjectsStore()

const title = computed(() => firstLine(props.todo.text) || t('item.untitled'))

const projectName = computed(
  () => projects.projects.find((p) => p.id === props.todo.project_id)?.name ?? t('focus.columns.noProject'),
)
const statusText = computed(() => (props.todo.done ? t('focus.actions.done') : t(`focus.status.${props.todo.status}`)))

/** The deadline relative to today; a finished todo is never late. */
const due = computed(() => {
  const date = props.todo.due_date
  const days = daysUntil(date, localYmd())
  if (!date || days == null) return null
  const label =
    days < 0 ? t('item.overdueDays', { n: -days }, -days)
    : days === 0 ? t('item.dueToday')
    : days === 1 ? t('item.dueTomorrow')
    : t('item.dueInDays', { n: days })
  return { label, status: props.todo.done ? 'later' : dueStatus(date, localYmd()) }
})

const meta = computed(() => {
  const parts = [t('item.metaCreated', { age: relativeAge(props.todo.created_at) })]
  const captured = props.links.find((l) => l.source === 'captured')
  if (captured) parts.push(t('item.metaFromSession', { name: runLabel(captured) }))
  return parts.join(' · ')
})
const metaTitle = computed(() => t('item.createdAt', { date: formatFullDate(props.todo.created_at) }))
</script>

<template>
  <div class="shrink-0 border-b border-border px-5 py-3">
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-md border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="todo.done ? 'border-primary bg-primary text-primary-foreground' : 'border-border bg-background text-transparent hover:border-primary hover:text-muted-foreground'"
        role="checkbox"
        :aria-checked="todo.done"
        :title="todo.done ? t('item.markNotDone') : t('item.markDone')"
        :aria-label="todo.done ? t('item.markNotDone') : t('item.markDone')"
        @click="emit('toggleDone')"
      >
        <Check class="h-4 w-4" :stroke-width="2.25" />
      </button>
      <p class="min-w-0 flex-1 truncate text-xs text-muted-foreground" :title="`${projectName} · ${statusText}`">
        {{ projectName }} · {{ statusText }}
      </p>

      <LinkedSessionsMenu
        class="shrink-0"
        :project-id="todo.project_id"
        :links="links"
        @open-run="emit('openRun', $event)"
        @unlink-run="emit('unlinkRun', $event)"
        @link="emit('linkRun', $event)"
      />
      <template v-if="editing">
        <Button variant="outline" size="sm" class="shrink-0" @click="emit('cancelEdit')">
          {{ t('common.cancel') }}
        </Button>
        <Button
          variant="primary"
          size="sm"
          class="shrink-0"
          :disabled="!canSave || saving"
          title="⌘/Ctrl+Enter"
          @click="emit('save')"
        >
          {{ t('common.save') }}
        </Button>
      </template>
      <template v-else>
        <Button variant="outline" size="sm" class="shrink-0" @click="emit('edit')">
          {{ t('item.editTitle') }}
        </Button>
        <DetailActionsMenu
          :new-session-label="t('item.newSessionTodo')"
          :new-session-hint="todo.project_id ? t('item.newSessionHint') : t('item.needProject')"
          :can-new-session="!!todo.project_id"
          :copied-id="copiedId"
          @new-session="emit('newSession')"
          @copy-id="emit('copyId')"
          @delete="emit('delete')"
        />
      </template>
      <Button
        v-if="closable"
        variant="ghost"
        size="icon"
        class="shrink-0"
        :title="t('focus.panel.close')"
        :aria-label="t('focus.panel.close')"
        @click="emit('close')"
      >
        <X class="h-4 w-4" :stroke-width="1.75" />
      </Button>
    </div>

    <h2
      class="mt-2 break-words text-base font-semibold leading-snug"
      :class="todo.done && 'text-muted-foreground line-through decoration-muted-foreground/60'"
    >
      {{ title }}
    </h2>
    <p class="mt-0.5 truncate text-xs text-muted-foreground" :title="metaTitle">
      {{ meta }}<template v-if="due">
        ·
        <span
          :class="{
            'font-medium text-destructive': due.status === 'overdue',
            'font-medium text-warning': due.status === 'today' || due.status === 'soon',
          }"
        >{{ due.label }}</span>
      </template>
    </p>
  </div>
</template>
