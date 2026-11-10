<script setup lang="ts">
// Composer button for the agent's checklist (its `TodoWrite` list): opens the
// newest list as editable rows — click the icon to change a status, edit the
// text, add or remove items. "Add to message" puts the edited list into the
// draft (lib/todoChecklist) so the agent gets it with the next message; nothing
// is sent from here, the same contract as CapturePicker.
import { computed, nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { CheckCircle2, Circle, CircleDot, ListTodo, Plus, X } from 'lucide-vue-next'
import { Button, DropdownMenu } from '@/components/ui'
import { nextStatus, normalizeTodos, sameTodos, type TodoItem, type TodoStatus } from '@/lib/todoChecklist'

const props = defineProps<{
  todos: TodoItem[]
  disabled?: boolean
}>()

const emit = defineEmits<{
  apply: [todos: TodoItem[]]
}>()

const { t } = useI18n()

const rows = ref<TodoItem[]>([])
const inputEls = ref<HTMLInputElement[]>([])

const done = computed(() => props.todos.filter((x) => x.status === 'completed').length)
const edited = computed(() => normalizeTodos(rows.value))
const changed = computed(() => !sameTodos(edited.value, props.todos))

const STATUS_LABEL: Record<TodoStatus, string> = {
  pending: 'run.checklist.statusPending',
  in_progress: 'run.checklist.statusInProgress',
  completed: 'run.checklist.statusCompleted',
}

// Every opening starts again from the newest list; an edit that was not added
// to the message is dropped, like a closed search box.
function onOpenChange(open: boolean) {
  if (open) rows.value = props.todos.map((x) => ({ ...x }))
}

function addRow() {
  rows.value.push({ content: '', status: 'pending' })
  nextTick(() => inputEls.value[rows.value.length - 1]?.focus())
}

// A disabled button lets the click through to the menu, which would close it.
function onApplyClick(e: MouseEvent) {
  if (!changed.value) {
    e.stopPropagation()
    return
  }
  emit('apply', edited.value)
}
</script>

<template>
  <DropdownMenu align="left" prefer="above" class="shrink-0" @update:open="onOpenChange">
    <template #trigger>
      <Button
        variant="ghost"
        size="sm"
        class="shrink-0 px-2"
        :disabled="disabled"
        :title="t('run.checklist.title')"
        :aria-label="t('run.checklist.title')"
      >
        <ListTodo class="h-3.5 w-3.5" :stroke-width="1.75" />
        <span class="text-2xs tabular-nums">{{ done }}/{{ todos.length }}</span>
      </Button>
    </template>

    <div class="w-96 max-w-[calc(100vw-2rem)]">
      <!-- Clicks stop here so editing doesn't close the menu. -->
      <div @click.stop>
        <div class="px-2 pt-1 pb-2">
          <div class="text-xs font-medium text-popover-foreground">{{ t('run.checklist.heading') }}</div>
          <div class="text-2xs text-muted-foreground">{{ t('run.checklist.hint') }}</div>
        </div>
        <div class="max-h-72 overflow-y-auto space-y-0.5">
          <p v-if="rows.length === 0" class="px-2.5 py-3 text-center text-2xs text-muted-foreground">
            {{ t('run.checklist.empty') }}
          </p>
          <div v-for="(row, i) in rows" :key="i" class="group flex items-center gap-1 px-1">
            <button
              type="button"
              class="h-6 w-6 shrink-0 flex items-center justify-center rounded hover:bg-accent cursor-pointer"
              :title="t(STATUS_LABEL[row.status])"
              :aria-label="t(STATUS_LABEL[row.status])"
              @click="row.status = nextStatus(row.status)"
            >
              <CheckCircle2 v-if="row.status === 'completed'" class="h-3.5 w-3.5 text-success" :stroke-width="1.75" />
              <CircleDot v-else-if="row.status === 'in_progress'" class="h-3.5 w-3.5 text-warning" :stroke-width="1.75" />
              <Circle v-else class="h-3.5 w-3.5 text-foreground/30" :stroke-width="1.75" />
            </button>
            <input
              :ref="(el) => { if (el) inputEls[i] = el as HTMLInputElement }"
              v-model="row.content"
              type="text"
              class="h-7 min-w-0 flex-1 rounded border border-transparent bg-transparent px-1.5 text-xs text-foreground outline-none hover:border-border focus:border-border focus:ring-1 focus:ring-ring"
              :class="{ 'line-through text-foreground/50': row.status === 'completed' }"
              :placeholder="t('run.checklist.itemPlaceholder')"
            >
            <button
              type="button"
              class="h-6 w-6 shrink-0 flex items-center justify-center rounded text-muted-foreground/60 hover:text-destructive hover:bg-destructive/10 cursor-pointer"
              :title="t('run.checklist.removeItem')"
              :aria-label="t('run.checklist.removeItem')"
              @click="rows.splice(i, 1)"
            >
              <X class="h-3 w-3" :stroke-width="2" />
            </button>
          </div>
        </div>
      </div>

      <div class="mt-1 flex items-center justify-between gap-2 border-t border-border px-1 pt-1.5 pb-0.5">
        <Button variant="ghost" size="xs" @click.stop="addRow">
          <Plus class="h-3 w-3" :stroke-width="2" />
          {{ t('run.checklist.addItem') }}
        </Button>
        <span class="inline-flex" @click="onApplyClick">
          <Button size="xs" :disabled="!changed">{{ t('run.checklist.apply') }}</Button>
        </span>
      </div>
    </div>
  </DropdownMenu>
</template>
