<script setup lang="ts">
// THE detail of a todo on the Tasks screen (V3-38), for both views: `dock` is
// the panel at the right of the Focus board (closable: button + Esc), `pane` is
// the part right of the list (always there, no close). Head (TaskDetailHeader),
// properties that save one by one (TaskFieldsGrid), the AI block
// (FocusAiPanel) and the markdown body (ItemBody), footer with the stamps.
//
// The panel owns reading / editing the body and the "Discard changes?" ask; the
// host asks `confirmDiscard()` before showing another todo, and `startEdit()`
// is how the host (keys, V3-39) starts an edit from outside. Writes go through
// the Focus store (`update_todo_fields`), then `changed` tells a host that
// keeps its own copy of the todos (the list) to reload.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import ItemBody from '@/components/items/ItemBody.vue'
import FocusAiPanel from '@/components/focus/FocusAiPanel.vue'
import FocusDialogs from '@/components/focus/FocusDialogs.vue'
import FocusAiDialogs from '@/components/focus/FocusAiDialogs.vue'
import { useFocusActions } from '@/components/focus/useFocusActions'
import { useFocusText } from '@/components/focus/useFocusText'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useFocusStore } from '@/stores/focus'
import { useProjectsStore } from '@/stores/projects'
import type { AiApply, FocusTodo, TodoPatch } from '@/lib/focus'
import { aiApplyPatch } from '@/lib/taskFieldPatch'
import TaskDetailHeader from './TaskDetailHeader.vue'
import TaskFieldsGrid from './TaskFieldsGrid.vue'
import { useTaskLinks } from './useTaskLinks'

const props = defineProps<{
  todo: FocusTodo | null
  mode: 'dock' | 'pane'
}>()

const emit = defineEmits<{
  /** Dock only: the user closed the panel (the edit, if any, was settled). */
  close: []
  /** The todo was deleted here. */
  deleted: []
  /** Something was written: a host with its own copy reloads it. */
  changed: []
}>()

const { t } = useI18n()
const { toast } = useToast()
const { confirm } = useConfirm()
const focus = useFocusStore()
const projects = useProjectsStore()
const actions = useFocusActions()
const { stamp } = useFocusText()

const todoRef = computed(() => props.todo)
const { links, copiedId, openRun, unlinkRun, linkRun, newSession, copyId } = useTaskLinks(todoRef)

onMounted(() => {
  if (!projects.projects.length) void projects.fetchProjects()
})

const effectiveEstimate = computed(
  () => focus.items.find((it) => it.todo.id === props.todo?.id)?.effective_estimate_min ?? null,
)

// ── Reading / editing the body ──
const editing = ref(false)
const bodyRef = ref<InstanceType<typeof ItemBody> | null>(null)
watch(() => props.todo?.id, () => (editing.value = false))

const hasUnsavedEdit = () => editing.value && !!bodyRef.value?.dirty

/** Leaving unsaved edits takes a confirm; true = go ahead. */
async function confirmDiscard(): Promise<boolean> {
  if (!hasUnsavedEdit()) return true
  return confirm({
    title: t('focus.panel.discardTitle'),
    message: t('item.confirm.discardMessage'),
    confirmLabel: t('item.confirm.discard'),
    cancelLabel: t('focus.panel.stay'),
  })
}

function startEdit() {
  if (props.todo) editing.value = true
}

async function cancelEdit() {
  if (await confirmDiscard()) editing.value = false
}

async function onBodySaved() {
  editing.value = false
  await focus.load()
  emit('changed')
}

async function requestClose() {
  if (!(await confirmDiscard())) return
  editing.value = false
  emit('close')
}

// ── Writes ──
/** One cell / one AI "Apply" = one `update_todo_fields`; false after a failure (toasted). */
async function save(patch: TodoPatch): Promise<boolean> {
  const td = props.todo
  if (!td) return false
  try {
    await focus.update(td.id, patch)
    emit('changed')
    return true
  } catch (e) {
    toast.error(String(e))
    return false
  }
}

function onAiApply(a: AiApply) {
  void save(aiApplyPatch(a))
}

async function toggleDone() {
  const td = props.todo
  if (!td) return
  if (td.done) await actions.undone(td)
  else await actions.done(td)
  emit('changed')
}

async function onDelete() {
  const td = props.todo
  if (!td || !(await actions.remove(td))) return
  editing.value = false
  emit('deleted')
  emit('changed')
}

// Esc closes the dock — unless something on top already used it (an open menu or dialog, an edit).
function onWindowKeydown(e: KeyboardEvent) {
  if (props.mode !== 'dock' || e.key !== 'Escape' || e.defaultPrevented || e.isComposing) return
  if (editing.value || document.querySelector('[role="dialog"], [role="menu"], [role="listbox"]')) return
  void requestClose()
}
onMounted(() => window.addEventListener('keydown', onWindowKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onWindowKeydown))

const stamps = computed(() => {
  const td = props.todo
  if (!td) return ''
  const parts = [t('focus.drawer.created', { time: stamp(td.created_at) })]
  if (td.started_at) parts.push(t('focus.drawer.started', { time: stamp(td.started_at) }))
  if (td.completed_at) parts.push(t('focus.drawer.completed', { time: stamp(td.completed_at) }))
  return parts.join(' · ')
})

defineExpose({ confirmDiscard, hasUnsavedEdit, startEdit })
</script>

<template>
  <section v-if="todo" class="flex h-full min-h-0 min-w-0 flex-1 flex-col bg-background">
    <TaskDetailHeader
      :todo="todo"
      :links="links"
      :editing="editing"
      :can-save="!!bodyRef?.canSave"
      :saving="!!bodyRef?.saving"
      :copied-id="copiedId"
      :closable="mode === 'dock'"
      @toggle-done="toggleDone"
      @edit="startEdit"
      @cancel-edit="cancelEdit"
      @save="bodyRef?.save()"
      @close="requestClose"
      @open-run="openRun"
      @unlink-run="unlinkRun"
      @link-run="linkRun"
      @new-session="newSession"
      @copy-id="copyId"
      @delete="onDelete"
    />

    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <TaskFieldsGrid :todo="todo" :effective-estimate="effectiveEstimate" :save="save" />
      <div class="shrink-0 border-b border-border/60 px-5 py-4">
        <FocusAiPanel :todo="todo" @apply="onAiApply" />
      </div>
      <div class="flex flex-1 flex-col" :class="editing ? 'min-h-[16rem]' : ''">
        <ItemBody
          ref="bodyRef"
          kind="todo"
          :item="todo"
          :editing="editing"
          @saved="onBodySaved"
          @cancel="cancelEdit"
        />
      </div>
      <p class="shrink-0 border-t border-border/60 px-5 py-2 text-2xs text-muted-foreground">{{ stamps }}</p>
    </div>

    <!-- The Focus board mounts these itself; the list has no other host for them. -->
    <template v-if="mode === 'pane'">
      <FocusDialogs />
      <FocusAiDialogs />
    </template>
  </section>
</template>
