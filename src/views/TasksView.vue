<script setup lang="ts">
// Tasks ("Việc"): one screen for the todos, in two views — Focus (the ranked board,
// views/FocusView.vue) and the list (a project-grouped list with the todo beside
// it, components/items/ItemListPane.vue) — under one header and one project /
// source filter. It is a screen of the main window (`/tasks?view=focus|list
// [&project=][&id=]`) or of its own window (`windowed`, views/FocusWindow.vue):
// lib/focusWindow keeps where the user left it, DetachButton / MergeBackButton move it.
//
// This screen also owns what both views need: the Focus store's feed (header
// count, calibration hint, the sidebar badge), and "show this todo" — from the
// URL, a reminder click (`focus:open-todo`, the pending id taken on mount) or
// another window re-pointing the window. A todo the Focus board does not hold (a
// done one) is shown in the list.
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Gauge } from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import { AppSelect, PageHeader, SegmentedControl } from '@/components/ui'
import AddTodoButton from '@/components/focus/AddTodoButton.vue'
import FocusDayPlan from '@/components/focus/FocusDayPlan.vue'
import DetachButton from '@/components/app/DetachButton.vue'
import MergeBackButton from '@/components/app/MergeBackButton.vue'
import ItemListPane from '@/components/items/ItemListPane.vue'
import FocusView from '@/views/FocusView.vue'
import { useFeaturesStore } from '@/stores/features'
import { useFocusStore } from '@/stores/focus'
import { useProjectsStore } from '@/stores/projects'
import {
  FILTER_NONE,
  FOCUS_OPEN_TODO,
  calibrationHint,
  isActiveBucket,
  type FocusFilter,
  type FocusOpenTodo,
} from '@/lib/focus'
import { TASKS_WINDOW_SELECT, openFocusWindow, tasksTarget } from '@/lib/focusWindow'
import {
  TASKS_PATH,
  isTaskView,
  parseTasksQuery,
  resolveTaskView,
  tasksPath,
  tasksQuery,
  visibleTaskViews,
  type TaskView,
  type TasksLocation,
} from '@/lib/tasksRoute'
import { TASKS_WINDOW_QUERY } from '@/lib/windowMode'

const props = defineProps<{
  /** Rendered in the Tasks window: no router, the address comes from the window's URL. */
  windowed?: boolean
}>()

const { t } = useI18n()
const store = useFocusStore()
const projects = useProjectsStore()
const features = useFeaturesStore()
const route = props.windowed ? null : useRoute()
const router = props.windowed ? null : useRouter()

// ── Where it opened, and what is remembered ──
const VIEW_KEY = 'puppeteer:tasksView'
const FILTER_KEY = 'puppeteer:focusFilter'

function readStored<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : fallback
  } catch {
    return fallback
  }
}
function writeStored(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* storage unavailable */
  }
}

const initial = parseTasksQuery(route ? route.query : TASKS_WINDOW_QUERY)
const savedView = ref<TaskView | null>((() => {
  const v = readStored<unknown>(VIEW_KEY, null)
  return isTaskView(v) ? v : null
})())
const wantedView = ref<TaskView | null>(initial.view)

// ── Views on offer (each follows its prop) ──
const visibleViews = computed(() =>
  visibleTaskViews(features.isOn('productivity.focus'), features.isOn('productivity.todos')),
)
const view = computed(() => resolveTaskView(wantedView.value, savedView.value, visibleViews.value))
const viewModel = computed<TaskView>({
  get: () => view.value ?? 'focus',
  set(v) {
    wantedView.value = v
    savedView.value = v
    writeStored(VIEW_KEY, v)
  },
})
const viewOptions = computed(() =>
  visibleViews.value.map((v) => ({ value: v, label: t(v === 'focus' ? 'tasks.viewFocus' : 'tasks.viewList') })),
)

// ── The filter both views share ──
const filter = reactive<FocusFilter>({ projectId: '', sourceType: '', ...readStored<Partial<FocusFilter>>(FILTER_KEY, {}) })
if (initial.projectId !== null) filter.projectId = initial.projectId
watch(filter, (f) => writeStored(FILTER_KEY, { projectId: f.projectId, sourceType: f.sourceType }))

const projectOptions = computed(() => [
  { value: '', label: t('focus.filters.allProjects') },
  ...projects.projects.map((p) => ({ value: p.id, label: p.name })),
  { value: FILTER_NONE, label: t('focus.filters.noProject') },
])
const SOURCES = ['slack', 'github', 'gitlab', 'run', 'url'] as const
const sourceOptions = computed(() => [
  { value: '', label: t('focus.filters.allSources') },
  ...SOURCES.map((s) => ({ value: s, label: t(`focus.sources.${s}`) })),
  { value: FILTER_NONE, label: t('focus.filters.noSource') },
])

// A todo's source, for the list (the todo rows carry none): read from the board.
const sourceById = computed(() => new Map(store.items.map((it) => [it.todo.id, it.todo.source_type])))
const sourceOf = (id: string) => sourceById.value.get(id) ?? null

// ── Header ──
const openCount = computed(() => store.items.filter((it) => isActiveBucket(it.bucket)).length)
// BR-10: shown only while calibration is on (the backend reports it with the board).
const calibration = computed(() => calibrationHint(store.board?.calibration))

// ── Show a todo ──
const focusRef = ref<InstanceType<typeof FocusView> | null>(null)
const listRef = ref<InstanceType<typeof ItemListPane> | null>(null)
const listInitialId = ref<string | null>(initial.id)
const selectedId = ref<string | null>(initial.id)

async function showTodo(id: string | null | undefined) {
  if (!id || !view.value) return
  if (view.value === 'focus') {
    await nextTick()
    const shown = await focusRef.value?.showTodo(id)
    if (shown === undefined || shown) return
    // Not on the board: the list shows it, when the list is on offer.
    if (!visibleViews.value.includes('list')) return
    viewModel.value = 'list'
  }
  listInitialId.value = id
  await nextTick()
  await listRef.value?.selectFresh(id)
}

function apply(loc: TasksLocation) {
  if (loc.view) wantedView.value = loc.view
  if (loc.projectId !== null && loc.projectId !== undefined) filter.projectId = loc.projectId
  void showTodo(loc.id)
}

// ── The address (main window): follows the view, filter and selected todo ──
function currentQuery() {
  return tasksQuery({
    view: view.value,
    projectId: filter.projectId,
    id: view.value === 'list' ? selectedId.value : null,
  })
}
function syncUrl() {
  if (!route || !router || route.path !== TASKS_PATH) return
  const q = currentQuery()
  const now = tasksQuery(parseTasksQuery(route.query))
  if (JSON.stringify(q) === JSON.stringify(now)) return
  router.replace({ path: TASKS_PATH, query: q }).catch(() => {})
}
if (route) {
  watch(
    () => route.fullPath,
    () => {
      if (route.path === TASKS_PATH) apply(parseTasksQuery(route.query))
    },
  )
  watch([view, () => filter.projectId, selectedId], syncUrl)
}

// ── Detach / merge back ──
const here = computed<TasksLocation>(() => ({
  view: view.value,
  projectId: filter.projectId,
  id: view.value === 'list' ? selectedId.value : null,
}))
const detachTarget = computed(() => ({ ...tasksTarget, openWindow: () => openFocusWindow(here.value) }))
const mergePath = computed(() => tasksPath(here.value))

// ── Lifecycle ──
let unlisten: UnlistenFn[] = []
onMounted(async () => {
  void store.start()
  if (!projects.projects.length) void projects.fetchProjects()
  syncUrl()
  try {
    unlisten.push(
      // Another window re-points this one (the Tasks window, opened again).
      await listen<TasksLocation>(TASKS_WINDOW_SELECT, (e) => {
        const p = e.payload ?? {}
        apply({ view: p.view ?? null, projectId: p.projectId ?? null, id: p.id ?? null })
      }),
      // A reminder click while this screen is open: consume the pending id too, so a later mount doesn't reopen it.
      await listen<FocusOpenTodo>(FOCUS_OPEN_TODO, (e) => {
        void invoke('focus_take_open_todo').catch(() => {})
        void showTodo(e.payload?.todo_id)
      }),
    )
    // The click that created this screen (or the window) left its todo pending.
    const pending = await invoke<string | null>('focus_take_open_todo')
    if (pending) void showTodo(pending)
    else if (initial.id && view.value === 'focus') void showTodo(initial.id)
  } catch {
    /* outside the Tauri shell */
  }
})
onBeforeUnmount(() => {
  store.stop()
  unlisten.forEach((off) => off())
  unlisten = []
})
</script>

<template>
  <div class="flex h-full flex-col bg-background">
    <PageHeader :title="t('tasks.title')" :count="store.loaded && openCount > 0 ? openCount : null" :count-title="t('tasks.countOpenTitle')">
      <template #meta>
        <SegmentedControl
          v-if="viewOptions.length > 1"
          v-model="viewModel"
          :options="viewOptions"
          :stretch="false"
          :aria-label="t('tasks.viewLabel')"
        />
        <span
          v-if="calibration"
          class="flex min-w-0 items-center gap-1 truncate text-2xs text-muted-foreground max-[901px]:hidden"
        >
          <Gauge class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          <span class="truncate">
            {{ calibration > 1 ? t('focus.calibration.slower', { factor: calibration }) : t('focus.calibration.faster', { factor: calibration }) }}
          </span>
        </span>
      </template>
      <template #actions>
        <FocusDayPlan v-if="view === 'focus'" @open="focusRef?.openTodo($event)" />
        <AddTodoButton button-variant="primary" size="sm" />
        <MergeBackButton v-if="windowed" screen="tasks" :path="mergePath" />
        <DetachButton v-else :target="detachTarget" />
      </template>
    </PageHeader>

    <!-- One filter for both views -->
    <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-border/60 px-6 py-2">
      <div class="w-44">
        <AppSelect v-model="filter.projectId" :options="projectOptions" size="xs" variant="outline" :aria-label="t('tasks.filterProject')" />
      </div>
      <div class="w-36">
        <AppSelect v-model="filter.sourceType" :options="sourceOptions" size="xs" variant="outline" :aria-label="t('tasks.filterSource')" />
      </div>
    </div>

    <FocusView v-if="view === 'focus'" ref="focusRef" class="min-h-0 flex-1" :filter="filter" />
    <div v-else-if="view === 'list'" class="min-h-0 flex-1">
      <ItemListPane
        ref="listRef"
        v-model:project="filter.projectId"
        kind="todo"
        embedded
        :source="filter.sourceType"
        :source-of="sourceOf"
        :initial-id="listInitialId"
        @selected="selectedId = $event"
      />
    </div>
  </div>
</template>
