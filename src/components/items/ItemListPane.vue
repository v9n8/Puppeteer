<script setup lang="ts">
// The body of the Todos / Notes list — sidebar of items, detail of the selected
// one — shared by its hosts: the Tasks screen (views/TasksView.vue, `embedded`,
// todos) and the Notes window (views/ItemListWindow.vue). A host picks the kind
// and may drive the project / source filter; windows add their own chrome.
//
//   ┌ sidebar: search · reload,              │ detail header: title /        │
//   │ segmented filter (notes All|Linked,    │ meta · [project][Sessions] |  │
//   │ todos Open|Done|All), (project filter  │ (✓ Done) ✎ (⧉) ⋯              │
//   │ + New unless embedded), items grouped  │ body: markdown, or the inline │
//   │ by project                             │ editor while editing          │
//   └────────────────────────────────────────┴───────────────────────────────┘
//
// Reading and editing both happen here: ✎ edits inline, ⌘/Ctrl+Enter saves,
// Esc cancels, clicking a note's title renames it (a todo: edits). Creating
// stays in the capture window (lib/itemWindow, "+ New").
//
// A window is its own webview with its own store copy: ITEM_CHANGED (from the
// capture window, the backend's link commands, the other windows) refetches; the
// MCP sidecar writes without events, so focusing the window reloads too.
// Sessions live in the main window: opening a linked one or starting a new one
// is a request to it (WINDOW_OPEN_RUN / WINDOW_NEW_SESSION, App.vue).
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ListTodo, StickyNote } from 'lucide-vue-next'
import AddTodoButton from '@/components/focus/AddTodoButton.vue'
import ItemSidebar from '@/components/items/ItemSidebar.vue'
import ItemHeader from '@/components/items/ItemHeader.vue'
import ItemBody from '@/components/items/ItemBody.vue'
import ItemFields from '@/components/items/ItemFields.vue'
import TaskDetailPanel from '@/components/tasks/TaskDetailPanel.vue'
import { useTaskKeyboard } from '@/components/tasks/useTaskKeyboard'
import FindBar from '@/components/FindBar.vue'
import { noteRow, todoRow, type ItemRow } from '@/components/items/itemRow'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { usePageFind } from '@/composables/usePageFind'
import { useProjectsStore } from '@/stores/projects'
import { useFeaturesStore } from '@/stores/features'
import { useNotesStore, type Note } from '@/stores/notes'
import { useTodosStore, type Todo } from '@/stores/todos'
import { matchesSource, type FocusTodo } from '@/lib/focus'
import { errorCode } from '@/stores/slackThreads'
import { ITEM_CHANGED, openItemCreateWindow, type ItemKind } from '@/lib/itemWindow'
import {
  WINDOW_NEW_SESSION,
  WINDOW_OPEN_RUN,
  linkReference,
  linkTargetRun,
  listTargetRuns,
  unlinkTargetRun,
  type SessionLink,
  type WindowNewSession,
  type WindowOpenRun,
} from '@/lib/sessionLinks'
import { firstLine } from '@/lib/itemText'
import { secretNoteFromNote, vaultError } from '@/lib/secretVault'
import { openSecretNotesFromPopout } from '@/lib/secretNotesWindow'

const { t } = useI18n()
const { toast } = useToast()
const { confirm } = useConfirm()
const notes = useNotesStore()
const todos = useTodosStore()
const projects = useProjectsStore()
const features = useFeaturesStore()

const props = defineProps<{
  /** Fixed for the host's lifetime. */
  kind: ItemKind
  /** Item to select once loaded. */
  initialId?: string | null
  /** The project the host was opened from: the default filter and "+ New" project. */
  contextProjectId?: string | null
  /** Inside the Tasks screen: its filter bar and header own the project filter, "New" and the pin. */
  embedded?: boolean
  /** Source filter of the Tasks screen ('' = all); read from each todo row. */
  source?: string
  pinned?: boolean
  /** The main window's Notes screen: no pin button. */
  hidePin?: boolean
}>()

const emit_ = defineEmits<{
  selected: [id: string | null]
  togglePin: []
}>()

/** The kind this pane shows — fixed for its lifetime. */
const kind: ItemKind = props.kind
const contextProjectId = ref<string | null>(props.contextProjectId ?? null)
/** "Make secret": into the vault, its own window. */
const canSecret = computed(() => kind === 'note' && features.allows('note.makeSecret'))

// ── Sidebar: search + project filter + kind filter (all client-side) ───────

const NO_PROJECT = '__none__'
const search = ref('')
/** '' = all projects, NO_PROJECT = items without one, else a project id. */
const projectFilter = defineModel<string>('project', { default: '' })
if (!props.embedded && !projectFilter.value) projectFilter.value = contextProjectId.value ?? ''
/** notes: '' | 'linked'; todos: 'active' | 'done' | 'all'. */
const filter = ref<string>(kind === 'note' ? '' : 'active')

const loadError = computed(() => (kind === 'note' ? notes.error : todos.error))

const allRows = computed<ItemRow[]>(() =>
  kind === 'note' ? notes.notes.map(noteRow) : todos.todos.map(todoRow),
)

const sourceById = computed(() => new Map(todos.todos.map((td) => [td.id, td.source_type])))

function matchesFilters(row: ItemRow): boolean {
  if (filter.value === 'linked' && row.linkedRunCount === 0) return false
  if (filter.value === 'active' && row.done) return false
  if (filter.value === 'done' && !row.done) return false
  return matchesScope(row)
}

/** The project / source filter alone (what the open count follows). */
function matchesScope(row: ItemRow): boolean {
  if (props.source && !matchesSource(sourceById.value.get(row.id), props.source)) return false
  if (projectFilter.value === NO_PROJECT) return !row.projectId
  if (projectFilter.value) return row.projectId === projectFilter.value
  return true
}

const openCount = computed(() => allRows.value.filter((r) => !r.done && matchesScope(r)).length)

const visibleRows = computed(() => {
  const q = search.value.trim().toLowerCase()
  return allRows.value.filter((row) => matchesFilters(row) && (!q || row.hay.includes(q)))
})

const projectNames = computed<Record<string, string>>(() =>
  Object.fromEntries(projects.projects.map((p) => [p.id, p.name])),
)

const projectOptions = computed(() => {
  const ids = new Set<string>()
  for (const row of allRows.value) if (row.projectId) ids.add(row.projectId)
  if (projectFilter.value && projectFilter.value !== NO_PROJECT) ids.add(projectFilter.value)
  const named = [...ids]
    .map((id) => ({ value: id, label: projectNames.value[id] ?? id }))
    .sort((a, b) => a.label.localeCompare(b.label))
  return [
    { value: '', label: t('item.allProjects') },
    ...named,
    { value: NO_PROJECT, label: t('item.noProject') },
  ]
})

const emptyStore = computed(() => allRows.value.length === 0)

// ── Selection ───────────────────────────────────────────────────────────────

const selectedId = ref<string | null>(null)
const item = computed<Note | Todo | null>(() => {
  const id = selectedId.value
  if (!id) return null
  return kind === 'note'
    ? notes.notes.find((n) => n.id === id) ?? null
    : todos.todos.find((x) => x.id === id) ?? null
})

/** Sessions linked to the selected item. */
const links = ref<SessionLink[]>([])
let linksSeq = 0
async function loadLinks() {
  const id = selectedId.value
  const seq = ++linksSeq
  if (!id) {
    links.value = []
    return
  }
  try {
    const list = await listTargetRuns({ kind, id })
    if (seq === linksSeq) links.value = list
  } catch {
    /* an extra: keep what is shown */
  }
}

const editing = ref(false)
const renaming = ref(false)
const bodyRef = ref<InstanceType<typeof ItemBody> | null>(null)
/** A todo is shown by the Tasks detail panel, which owns its own editing. */
const panelRef = ref<InstanceType<typeof TaskDetailPanel> | null>(null)
const todoItem = computed(() => (kind === 'todo' ? (item.value as FocusTodo | null) : null))

function onPanelChanged() {
  void refresh()
  announce()
}

// ⌘F searches the item as it reads (not its edit fields); another item or an
// edit re-renders it, which detaches the old matches.
const bodyWrapEl = ref<HTMLElement | null>(null)
const find = usePageFind(computed(() => (editing.value ? null : bodyWrapEl.value)))
const findOpen = find.open
watch([item, editing], () => (editing.value ? find.closeFind() : find.refresh()))

/** Leaving unsaved edits takes a confirm; true = go ahead. */
async function confirmDiscard(): Promise<boolean> {
  if (kind === 'todo') return (await panelRef.value?.confirmDiscard()) ?? true
  if (!editing.value || !bodyRef.value?.dirty) return true
  return confirm({
    title: t('item.confirm.discardTitle'),
    message: t('item.confirm.discardMessage'),
    confirmLabel: t('item.confirm.discard'),
  })
}

async function select(id: string | null) {
  if (id === selectedId.value) return
  if (!(await confirmDiscard())) return
  editing.value = false
  renaming.value = false
  selectedId.value = id
  if (id && !visibleRows.value.some((r) => r.id === id)) revealRow(id)
  links.value = []
  void loadLinks()
  emit_('selected', id)
}

/** Make a row visible under the filters (an item opened from elsewhere). */
function revealRow(id: string) {
  const row = allRows.value.find((r) => r.id === id)
  if (!row) return
  search.value = ''
  if (!matchesFilters(row)) {
    projectFilter.value = ''
    filter.value = kind === 'note' ? '' : 'all'
  }
}

// The selected item vanished (deleted here, elsewhere or by the AI). An
// unsaved draft is not dropped silently: it goes to the clipboard.
watch(item, (now, before) => {
  if (!now && before && selectedId.value) {
    if (editing.value && bodyRef.value?.dirty) void rescueDraft(bodyRef.value.draftText())
    editing.value = false
    selectedId.value = visibleRows.value[0]?.id ?? null
    void loadLinks()
    emit_('selected', selectedId.value)
  }
})

// A filter or search that hides the selected row clears the detail, so it never shows
// something the list no longer has. An item being edited stays until the edit ends.
watch(visibleRows, (rows) => {
  if (!ready.value || !selectedId.value || editing.value) return
  if (!rows.some((r) => r.id === selectedId.value)) void select(null)
})

async function rescueDraft(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    toast.info(t('item.deletedNoteDraftCopied'))
  } catch {
    toast.error(t('item.deletedNoteDraftLost'))
  }
}

// ── Writes ──────────────────────────────────────────────────────────────────

/** Tell the other webviews (other windows, the main window's picker / menus). */
function announce() {
  emit(ITEM_CHANGED, { kind }).catch(() => {})
}

function startEdit() {
  if (!item.value) return
  renaming.value = false
  editing.value = true
}

// Esc / Cancel: a stray Esc must not silently drop a hand-written draft.
async function cancelEdit() {
  if (!(await confirmDiscard())) return
  editing.value = false
}

function onSaved() {
  editing.value = false
  announce()
}

function startRename() {
  if (!item.value) return
  renaming.value = true
}

async function commitRename(title: string) {
  renaming.value = false
  const n = item.value as Note | null
  if (!n || !title || title === n.title) return
  await notes.update(n.id, title, n.content)
  announce()
}

async function setProject(next: string | null) {
  const it = item.value
  if (!it || next === it.project_id) return
  await notes.setProject(it.id, next)
  announce()
}

async function toggleDone(id: string) {
  if (kind !== 'todo') return
  await todos.toggle(id)
  announce()
}

async function remove() {
  const it = item.value
  if (!it) return
  // Only notes get here: a todo is deleted from its detail panel.
  const ok = await confirm({
    title: t('item.confirm.deleteNoteTitle'),
    message: t('item.confirm.deleteNoteMessage'),
    confirmLabel: t('common.delete'),
  })
  if (!ok) return
  // The next row takes the selection.
  const rows = visibleRows.value
  const i = rows.findIndex((r) => r.id === it.id)
  const next = rows[i + 1]?.id ?? rows[i - 1]?.id ?? null
  editing.value = false
  await notes.remove(it.id)
  selectedId.value = next
  void loadLinks()
  emit_('selected', next)
  announce()
}

/**
 * Delete a folder as the sidebar shows it: the items of the project (null =
 * "No project") that pass the current filters and search — hidden ones stay.
 */
async function removeGroup(projectId: string | null, label: string) {
  const ids = visibleRows.value.filter((r) => (r.projectId ?? null) === projectId).map((r) => r.id)
  if (!ids.length) return
  const isNote = kind === 'note'
  const ok = await confirm({
    title: isNote ? t('item.confirm.deleteNoteFolderTitle') : t('item.confirm.deleteTodoFolderTitle'),
    message: isNote
      ? t('item.confirm.deleteNoteFolderMessage', { n: ids.length, folder: label }, ids.length)
      : t('item.confirm.deleteTodoFolderMessage', { n: ids.length, folder: label }, ids.length),
    confirmLabel: t('common.delete'),
    variant: 'destructive',
  })
  if (!ok) return
  const gone = new Set(ids)
  if (selectedId.value && gone.has(selectedId.value)) {
    // The open item goes too: its unsaved draft is dropped with it (confirmed above).
    editing.value = false
    renaming.value = false
    selectedId.value = visibleRows.value.find((r) => !gone.has(r.id))?.id ?? null
    void loadLinks()
  }
  const store = isNote ? notes : todos
  store.error = null
  if (isNote) await notes.removeMany(ids)
  else await todos.removeMany(ids)
  if (store.error) toast.error(t('item.deleteFolderFailed', { error: store.error }))
  else toast.success(t('item.folderDeleted', { n: ids.length }, ids.length))
  announce()
}

/** Move the selected note into the vault (the vault must be unlocked), then show it there. */
async function makeSecret() {
  const it = item.value
  if (!it || kind !== 'note') return
  const ok = await confirm({
    title: t('secretNotes.makeSecret.title'),
    message: t('secretNotes.makeSecret.message'),
    confirmLabel: t('secretNotes.makeSecret.confirm'),
  })
  if (!ok) return
  try {
    const moved = await secretNoteFromNote(it.id)
    editing.value = false
    await notes.fetchNotes()
    toast.success(t('secretNotes.makeSecret.done'))
    void openSecretNotesFromPopout({ id: moved.id })
  } catch (e) {
    const { code } = vaultError(e)
    if (code === 'vault_locked' || code === 'vault_missing') {
      void openSecretNotesFromPopout({ projectId: it.project_id })
      toast.info(t(`secretNotes.makeSecret.${code === 'vault_locked' ? 'needUnlock' : 'needVault'}`))
    } else {
      toast.error(code ? t(`secretNotes.errors.${code}`) : String(e))
    }
  }
}

const copiedId = ref(false)
async function copyId() {
  const id = selectedId.value
  if (!id) return
  try {
    await navigator.clipboard.writeText(id)
    copiedId.value = true
    setTimeout(() => (copiedId.value = false), 1500)
    toast.success(t('item.idCopied'))
  } catch {
    /* clipboard unavailable */
  }
}

function openCreate() {
  const scoped = projectFilter.value && projectFilter.value !== NO_PROJECT ? projectFilter.value : null
  void openItemCreateWindow(kind, { projectId: scoped ?? contextProjectId.value })
}

// ── Linked sessions ─────────────────────────────────────────────────────────

function openRun(link: SessionLink) {
  emit(WINDOW_OPEN_RUN, { projectId: link.projectId, runId: link.runId } satisfies WindowOpenRun).catch(
    () => {},
  )
}

async function unlinkRun(link: SessionLink) {
  const id = selectedId.value
  if (!id) return
  const snapshot = links.value.slice()
  links.value = links.value.filter((l) => l.runId !== link.runId)
  try {
    await unlinkTargetRun({ kind, id }, link.runId)
  } catch (e) {
    links.value = snapshot
    toast.error(t('sessionLink.unlinkFailed', { error: errorCode(e).detail }))
  }
}

async function linkRun(runId: string) {
  const id = selectedId.value
  if (!id) return
  try {
    await linkTargetRun({ kind, id }, runId, 'manual')
    await loadLinks()
  } catch (e) {
    toast.error(t('sessionLink.linkFailed', { error: errorCode(e).detail }))
  }
}

/**
 * Ask the main window for a new session in the item's project, its composer
 * prefilled with the item's reference; it links the item once the run exists.
 */
function newSession() {
  const it = item.value
  if (!it) return
  if (!it.project_id) {
    toast.info(t('item.needProject'))
    return
  }
  const n = it as Note
  const title = n.title.trim() || firstLine(n.content) || t('item.untitled')
  emit(WINDOW_NEW_SESSION, {
    projectId: it.project_id,
    text: linkReference(kind, it.id, title),
    target: { kind, id: it.id },
  } satisfies WindowNewSession).catch((e) => toast.error(String(e)))
}

// ── Reload / events ─────────────────────────────────────────────────────────

/** The first load is done (until then the sidebar shows placeholders). */
const ready = ref(false)
const reloading = ref(false)
async function refresh() {
  if (kind === 'note') await notes.fetchNotes()
  else await todos.fetchTodos()
}

async function reloadAll() {
  reloading.value = true
  try {
    await Promise.all([refresh(), loadLinks()])
  } finally {
    reloading.value = false
  }
}

let reloadTimer: ReturnType<typeof setTimeout> | null = null
function scheduleReload() {
  if (reloadTimer) clearTimeout(reloadTimer)
  reloadTimer = setTimeout(() => void reloadAll(), 250)
}

// The host window's title, which doubles as its row in the menu-bar switcher
// (lib/tray.ts): "Notes: <title>" for the open note, "Todos" for the todos.
const windowTitle = computed(() => {
  if (kind === 'todo') return t('item.navTodos')
  const n = item.value as Note | null
  const name = n ? n.title.trim() || firstLine(n.content) : ''
  return name ? t('item.notesWindowWith', { name }) : t('item.navNotes')
})
let unlisten: UnlistenFn[] = []

/**
 * Select an item asked for from elsewhere: one created a moment ago may not be
 * in this window's store yet, so refetch first.
 */
async function selectFresh(id: string) {
  if (!allRows.value.some((r) => r.id === id)) await refresh()
  await select(id)
}

onMounted(async () => {
  void projects.fetchProjects()
  try {
    unlisten.push(
      // A write elsewhere (capture window, link commands, other windows).
      await listen<{ kind?: string }>(ITEM_CHANGED, (e) => {
        if (!e.payload?.kind || e.payload.kind === kind) scheduleReload()
      }),
    )
    // MCP sidecar writes (notes_* / todos_*) emit no Tauri event.
    unlisten.push(
      await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (focused && !editing.value) void reloadAll()
      }),
    )
  } catch {
    /* running outside the Tauri shell */
  }
  await refresh()
  await nextTick()
  // A stale `id` (deleted since) falls back to the first row.
  const wanted = props.initialId && allRows.value.some((r) => r.id === props.initialId) ? props.initialId : null
  const first = selectedId.value ?? wanted ?? visibleRows.value[0]?.id ?? null
  if (first) await select(first)
  ready.value = true
})

/** Point the pane at a project (null = leave the scope alone) and / or an item, as the host is re-opened. */
function setScope(projectId: string | null | undefined) {
  if (projectId === undefined) return
  contextProjectId.value = projectId ?? null
  projectFilter.value = projectId ?? ''
}

// ── Keys of the todo list (V3-39): ↑ ↓ pick, Space done, E edit, Enter open ──
const sidebarRef = ref<InstanceType<typeof ItemSidebar> | null>(null)
const rowButton = (id: string) =>
  (sidebarRef.value?.$el as HTMLElement | undefined)?.querySelector<HTMLElement>(`[data-task-row="${CSS.escape(id)}"]`) ?? null

async function pickRow(id: string) {
  await select(id)
  if (selectedId.value !== id) return
  await nextTick()
  const el = rowButton(id)
  el?.focus({ preventScroll: true })
  el?.scrollIntoView({ block: 'nearest' })
}
async function editRow(id: string) {
  await pickRow(id)
  if (selectedId.value === id) panelRef.value?.startEdit()
}
const keyboard = useTaskKeyboard({
  columns: () => [
    [...((sidebarRef.value?.$el as HTMLElement | undefined)?.querySelectorAll<HTMLElement>('[data-task-row]') ?? [])].map(
      (el) => el.dataset.taskRow as string,
    ),
  ],
  week: () => false,
  selectedId,
  todoOf: (id) => (kind === 'todo' ? ((todos.todos.find((x) => x.id === id) as FocusTodo | undefined) ?? null) : null),
  select: pickRow,
  open: pickRow,
  edit: editRow,
  changed: onPanelChanged,
})
function onSidebarKeydown(e: KeyboardEvent) {
  if (kind === 'todo') keyboard.onKeydown(e)
}

defineExpose({
  select,
  selectFresh,
  setScope,
  confirmDiscard,
  /** An edit with unsaved changes is open. */
  hasUnsavedEdit: () => (kind === 'todo' ? !!panelRef.value?.hasUnsavedEdit() : editing.value && !!bodyRef.value?.dirty),
  windowTitle,
})

onBeforeUnmount(() => {
  if (reloadTimer) clearTimeout(reloadTimer)
  unlisten.forEach((off) => off())
  unlisten = []
})
</script>

<template>
  <div class="flex h-full min-h-0 w-full overflow-hidden bg-background text-foreground">
    <ItemSidebar
      ref="sidebarRef"
      v-model:search="search"
      v-model:project="projectFilter"
      v-model:filter="filter"
      :kind="kind"
      :rows="visibleRows"
      :empty-store="emptyStore"
      :selected-id="selectedId"
      :project-options="projectOptions"
      :project-names="projectNames"
      :error="loadError"
      :pinned="!!pinned"
      :hide-pin="hidePin"
      :reloading="reloading"
      :embedded="embedded"
      :open-count="openCount"
      :loading="!ready"
      @select="select"
      @keydown="onSidebarKeydown"
      @add="openCreate"
      @toggle-done="toggleDone"
      @delete-group="removeGroup"
      @toggle-pin="emit_('togglePin')"
      @reload="reloadAll"
      @retry="reloadAll"
    />
    <TaskDetailPanel v-if="todoItem" ref="panelRef" :todo="todoItem" mode="pane" @changed="onPanelChanged" />
    <section v-else-if="item" class="flex min-w-0 flex-1 flex-col">
      <ItemHeader
        :item="item as Note"
        :links="links"
        :renaming="renaming"
        :editing="editing"
        :can-save="!!bodyRef?.canSave"
        :saving="!!bodyRef?.saving"
        :copied-id="copiedId"
        :can-make-secret="canSecret"
        @start-rename="startRename"
        @commit-rename="commitRename"
        @cancel-rename="renaming = false"
        @edit="startEdit"
        @cancel-edit="cancelEdit"
        @save="bodyRef?.save()"
        @open-run="openRun"
        @unlink-run="unlinkRun"
        @link-run="linkRun"
        @new-session="newSession"
        @copy-id="copyId"
        @make-secret="makeSecret"
        @delete="remove"
      />
      <ItemFields :item="item as Note" @set-project="setProject" />
      <!-- Zero-height anchor: the ⌘F bar floats over the body's top edge. -->
      <div v-if="findOpen" class="relative z-20 h-0 shrink-0">
        <FindBar :find="find" class="absolute top-2 right-4" />
      </div>
      <div ref="bodyWrapEl" class="flex min-h-0 flex-1 flex-col">
        <ItemBody
          ref="bodyRef"
          :kind="kind"
          :item="item"
          :editing="editing"
          @saved="onSaved"
          @cancel="cancelEdit"
        />
      </div>
    </section>
    <div v-else class="flex min-w-0 flex-1 items-center justify-center p-6">
      <div class="flex flex-col items-center text-center">
        <div class="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-xl bg-muted">
          <component
            :is="kind === 'note' ? StickyNote : ListTodo"
            class="h-6 w-6 text-muted-foreground"
            :stroke-width="1.5"
          />
        </div>
        <template v-if="embedded && emptyStore && ready">
          <p class="text-sm font-medium">{{ t('tasks.emptyTitle') }}</p>
          <AddTodoButton class="mt-3" button-variant="primary" size="sm" />
        </template>
        <p v-else class="text-xs text-muted-foreground">
          {{ kind === 'note' ? t('item.noSelectionNote') : t('tasks.pickOne') }}
        </p>
      </div>
    </div>

  </div>
</template>
