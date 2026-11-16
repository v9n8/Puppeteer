<script setup lang="ts">
// Properties of a todo in the task detail panel (V3-38), a two-column grid.
// Every cell saves the moment it changes — `save(patch)` is one
// `update_todo_fields` call (the host's); when it fails the cell goes back to
// the stored value (the host shows the toast). Typed cells (estimate, time,
// reminder…) save on blur / Enter / picking; a text that can't be read stays
// open with a red hint and sends nothing.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ExternalLink, X } from 'lucide-vue-next'
import { AppSelect, DatePicker, Input } from '@/components/ui'
import ProjectSelect from '@/components/ProjectSelect.vue'
import { SOURCE_ICON } from '@/components/focus/sourceIcon'
import { useFocusActions } from '@/components/focus/useFocusActions'
import {
  formatEstimate,
  sourceShortLabel,
  toLocalInput,
  type FocusTodo,
  type TodoPatch,
  type TodoPriority,
  type TodoStatus,
} from '@/lib/focus'
import {
  actualPatch,
  dueDatePatch,
  dueTimePatch,
  estimatePatch,
  instantPatch,
  type FieldResult,
} from '@/lib/taskFieldPatch'

const props = defineProps<{
  todo: FocusTodo
  /** What the board counts for this todo (differs from the estimate when it has none). */
  effectiveEstimate: number | null
  /** Resolves true when stored, false after a failure (already toasted). */
  save: (patch: TodoPatch) => Promise<boolean>
}>()

const { t } = useI18n()
const actions = useFocusActions()

const PRIORITIES: TodoPriority[] = [1, 2, 3]
const STATUSES: TodoStatus[] = ['todo', 'doing', 'waiting']
const statusOptions = computed(() => STATUSES.map((s) => ({ value: s, label: t(`focus.status.${s}`) })))

// ── Typed cells keep a local text until they are saved ──
const estimate = ref('')
const actual = ref('')
const dueTime = ref('')
const snoozedUntil = ref('')
const remindAt = ref('')
const invalid = ref<Record<string, boolean>>({})

function load() {
  const td = props.todo
  estimate.value = formatEstimate(td.estimate_min)
  actual.value = formatEstimate(td.actual_min)
  dueTime.value = td.due_time ?? ''
  snoozedUntil.value = toLocalInput(td.snoozed_until)
  remindAt.value = toLocalInput(td.remind_at)
  invalid.value = {}
}
// Another todo, or the stored value moved (a save, the board refreshing): show it.
watch(
  () => [props.todo.id, props.todo.estimate_min, props.todo.actual_min, props.todo.due_time, props.todo.snoozed_until, props.todo.remind_at],
  load,
  { immediate: true },
)

async function commit(cell: string, result: FieldResult) {
  invalid.value = { ...invalid.value, [cell]: !result.ok }
  if (!result.ok || Object.keys(result.patch).length === 0) return
  if (!(await props.save(result.patch))) load()
}

const projectId = computed({
  get: () => props.todo.project_id,
  set: (v: string | null) => {
    if (v !== props.todo.project_id) void props.save({ project_id: v })
  },
})
const status = computed({
  get: () => props.todo.status,
  set: (v: TodoStatus) => {
    if (v !== props.todo.status) void props.save({ status: v })
  },
})

function setPriority(p: TodoPriority) {
  if (p !== props.todo.priority) void props.save({ priority: p })
}

const sourceText = computed(() => {
  const td = props.todo
  if (!td.source_url) return ''
  return sourceShortLabel(td.source_type, td.source_ref, td.source_url) ?? td.source_ref ?? td.source_url
})

const labelClass = 'block text-2xs font-medium text-muted-foreground'
const clearBtn =
  'flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'
</script>

<template>
  <div class="grid shrink-0 grid-cols-2 gap-x-3 gap-y-3 border-b border-border/60 px-5 py-4">
    <div class="space-y-1">
      <label :class="labelClass">{{ t('focus.drawer.project') }}</label>
      <ProjectSelect v-model="projectId" variant="default" size="sm" />
    </div>
    <div class="space-y-1">
      <label :class="labelClass">{{ t('focus.status.label') }}</label>
      <AppSelect v-model="status" :options="statusOptions" :disabled="todo.done" />
    </div>

    <div class="space-y-1">
      <label :class="labelClass">{{ t('focus.capture.due') }}</label>
      <div class="flex items-center gap-1">
        <DatePicker
          :model-value="todo.due_date ?? ''"
          size="sm"
          class="min-w-0 flex-1"
          :placeholder="t('focus.capture.due')"
          @update:model-value="commit('due', dueDatePatch($event, todo))"
        />
        <button
          v-if="todo.due_date"
          type="button"
          :class="clearBtn"
          :title="t('focus.drawer.clearDue')"
          :aria-label="t('focus.drawer.clearDue')"
          @click="commit('due', dueDatePatch('', todo))"
        >
          <X class="h-3.5 w-3.5" :stroke-width="2" />
        </button>
      </div>
    </div>
    <div class="space-y-1">
      <label :class="labelClass">{{ t('focus.drawer.dueTime') }}</label>
      <Input
        v-model="dueTime"
        type="time"
        size="sm"
        class="w-full"
        :disabled="!todo.due_date"
        @change="commit('dueTime', dueTimePatch(dueTime, todo))"
      />
    </div>

    <div class="space-y-1">
      <label :class="labelClass">{{ t('focus.capture.priority') }}</label>
      <div class="flex h-8 overflow-hidden rounded-md border border-border" role="radiogroup" :aria-label="t('focus.capture.priority')">
        <button
          v-for="p in PRIORITIES"
          :key="p"
          type="button"
          role="radio"
          :aria-checked="todo.priority === p"
          :title="t(`focus.priority.${p}`)"
          class="flex-1 cursor-pointer text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring not-first:border-l not-first:border-border"
          :class="todo.priority === p
            ? p === 1 ? 'bg-destructive text-destructive-foreground' : 'bg-primary text-primary-foreground'
            : 'bg-background text-muted-foreground hover:bg-accent hover:text-foreground'"
          @click="setPriority(p)"
        >
          P{{ p }}
        </button>
      </div>
    </div>
    <div class="space-y-1">
      <label :class="labelClass">{{ t('focus.capture.estimate') }}</label>
      <Input
        v-model="estimate"
        size="sm"
        class="w-full"
        :class="invalid.estimate ? 'border-destructive focus-visible:ring-destructive' : ''"
        :placeholder="t('focus.drawer.estimatePlaceholder')"
        :aria-invalid="!!invalid.estimate"
        @blur="commit('estimate', estimatePatch(estimate, todo))"
        @keydown.enter.prevent="commit('estimate', estimatePatch(estimate, todo))"
      />
    </div>
    <p v-if="invalid.estimate" class="col-span-2 -mt-2 text-2xs text-destructive">{{ t('focus.drawer.estimateInvalid') }}</p>
    <p v-else-if="effectiveEstimate != null && effectiveEstimate !== todo.estimate_min" class="col-span-2 -mt-2 text-2xs text-muted-foreground">
      {{ t('focus.drawer.effectiveEstimate', { estimate: formatEstimate(effectiveEstimate) }) }}
      <template v-if="todo.estimate_source === 'ai'"> · {{ t('focus.estimate.ai') }}</template>
    </p>
    <p v-else-if="todo.estimate_source === 'ai'" class="col-span-2 -mt-2 text-2xs text-muted-foreground">{{ t('focus.estimate.ai') }}</p>

    <div class="space-y-1">
      <label :class="labelClass">{{ t('focus.drawer.snoozedUntil') }}</label>
      <div class="flex items-center gap-1">
        <Input
          v-model="snoozedUntil"
          type="datetime-local"
          size="sm"
          class="min-w-0 flex-1"
          @change="commit('snooze', instantPatch('snoozed_until', snoozedUntil, todo))"
        />
        <button
          v-if="snoozedUntil"
          type="button"
          :class="clearBtn"
          :title="t('focus.drawer.clear')"
          :aria-label="t('focus.drawer.clear')"
          @click="snoozedUntil = ''; commit('snooze', instantPatch('snoozed_until', '', todo))"
        >
          <X class="h-3.5 w-3.5" :stroke-width="2" />
        </button>
      </div>
    </div>
    <div class="space-y-1">
      <label :class="labelClass">{{ t('focus.drawer.remindAt') }}</label>
      <div class="flex items-center gap-1">
        <Input
          v-model="remindAt"
          type="datetime-local"
          size="sm"
          class="min-w-0 flex-1"
          @change="commit('remind', instantPatch('remind_at', remindAt, todo))"
        />
        <button
          v-if="remindAt"
          type="button"
          :class="clearBtn"
          :title="t('focus.drawer.clear')"
          :aria-label="t('focus.drawer.clear')"
          @click="remindAt = ''; commit('remind', instantPatch('remind_at', '', todo))"
        >
          <X class="h-3.5 w-3.5" :stroke-width="2" />
        </button>
      </div>
    </div>

    <div v-if="todo.done || todo.started_at || todo.actual_min != null" class="space-y-1">
      <label :class="labelClass">{{ t('focus.drawer.actual') }}</label>
      <Input
        v-model="actual"
        size="sm"
        class="w-full"
        :class="invalid.actual ? 'border-destructive focus-visible:ring-destructive' : ''"
        :placeholder="t('focus.drawer.estimatePlaceholder')"
        :aria-invalid="!!invalid.actual"
        @blur="commit('actual', actualPatch(actual, todo))"
        @keydown.enter.prevent="commit('actual', actualPatch(actual, todo))"
      />
      <p v-if="invalid.actual" class="text-2xs text-destructive">{{ t('focus.drawer.estimateInvalid') }}</p>
    </div>

    <div v-if="todo.source_url" class="space-y-1">
      <label :class="labelClass">{{ t('focus.drawer.source') }}</label>
      <button
        type="button"
        class="inline-flex h-8 max-w-full cursor-pointer items-center gap-1.5 rounded-md border border-border px-2 text-xs transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :title="todo.source_url"
        @click="actions.openSource(todo)"
      >
        <component :is="SOURCE_ICON[todo.source_type ?? 'url']" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
        <span class="min-w-0 truncate">{{ sourceText }}</span>
        <ExternalLink class="h-3 w-3 shrink-0 text-muted-foreground" :stroke-width="1.75" />
      </button>
    </div>
  </div>
</template>
