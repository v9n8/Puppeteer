<script setup lang="ts">
// Focus board (docs/focus-board-spec.md §3.2): every open todo of every project,
// ranked by the backend (BR-04/05) — the UI only groups by bucket and sorts by
// score. Three views: by time ("Do now" top 3 + collapsible buckets), by project
// (one column each), and the week (Mon → Sun columns by due date). Rows open the
// task detail panel (components/tasks/TaskDetailPanel.vue, docked at the right). The screen around it (views/TasksView.vue) owns the header, the
// project / source filter (`filter`) and the data feed (store start / stop, reminder
// clicks); it opens a todo here through `showTodo` — panel open, row flashing.
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { CalendarClock, ChevronDown, ChevronRight, RefreshCw, Target, TriangleAlert } from 'lucide-vue-next'
import { Banner, Button, Card, SegmentedControl, Skeleton } from '@/components/ui'
import AddTodoButton from '@/components/focus/AddTodoButton.vue'
import FocusDialogs from '@/components/focus/FocusDialogs.vue'
import FocusAiDialogs from '@/components/focus/FocusAiDialogs.vue'
import TaskDetailPanel from '@/components/tasks/TaskDetailPanel.vue'
import FocusTodoRow from '@/components/focus/FocusTodoRow.vue'
import { useTaskKeyboard } from '@/components/tasks/useTaskKeyboard'
import { useWeekDrag } from '@/components/tasks/useWeekDrag'
import { useToast } from '@/composables/useToast'
import { useFocusText } from '@/components/focus/useFocusText'
import { useFeaturesStore } from '@/stores/features'
import { useFocusStore } from '@/stores/focus'
import { useProjectsStore } from '@/stores/projects'
import { moveDueDate } from '@/lib/taskKeys'
import {
  CAPACITY_LOADING,
  CAPACITY_NO_ACCOUNT,
  DEFAULT_OPEN_BUCKETS,
  filterItems,
  formatEstimate,
  groupByBucket,
  groupByProject,
  groupByWeek,
  isActiveBucket,
  projectHue,
  topItems,
  ymdOf,
  type FocusBucket,
  type FocusFilter,
} from '@/lib/focus'

type FocusViewMode = 'time' | 'project' | 'week'

const props = defineProps<{ filter: FocusFilter }>()

const { t, locale } = useI18n()
const store = useFocusStore()
const projects = useProjectsStore()
const { span } = useFocusText()
const { toast } = useToast()

// ── View + filters (remembered) ──
const VIEW_KEY = 'puppeteer:focusView'
const OPEN_KEY = 'puppeteer:focusOpenBuckets'

const VIEWS: FocusViewMode[] = ['time', 'project', 'week']
const view = ref<FocusViewMode>(VIEWS.find((v) => v === localStorage.getItem(VIEW_KEY)) ?? 'time')
watch(view, (v) => localStorage.setItem(VIEW_KEY, v))
const viewOptions = computed(() => VIEWS.map((v) => ({ value: v, label: t(`focus.views.${v}`) })))

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : fallback
  } catch {
    return fallback
  }
}
const openBuckets = ref(new Set<FocusBucket>(readJson<FocusBucket[]>(OPEN_KEY, DEFAULT_OPEN_BUCKETS)))
function toggleBucket(b: FocusBucket) {
  const next = new Set(openBuckets.value)
  if (next.has(b)) next.delete(b)
  else next.add(b)
  openBuckets.value = next
  localStorage.setItem(OPEN_KEY, JSON.stringify([...next]))
}

// ── Board ──
const now = computed(() => store.now)
const visible = computed(() => filterItems(store.items, props.filter))
const top = computed(() => topItems(visible.value, store.board?.top ?? []))
const buckets = computed(() => groupByBucket(visible.value))
const projectColumns = computed(() => groupByProject(visible.value))
const weekColumns = computed(() => groupByWeek(visible.value, now.value))
const today = computed(() => ymdOf(now.value))

const capacity = computed(() => store.board?.capacity ?? null)
/** Why there is no capacity bar: no Google account → connect hint; other errors → retry. */
const capacityError = computed(() => (store.board && !store.board.capacity ? store.board.capacity_error ?? null : null))
// Calendar or Google off: the backend counts no busy time (capacity degraded).
const features = useFeaturesStore()
const calendarOff = computed(
  () => features.loaded && !(features.isOn('productivity.calendar') && features.isOn('integrations.google')),
)
const refreshingCalendar = ref(false)
async function refreshCalendar() {
  if (refreshingCalendar.value) return
  refreshingCalendar.value = true
  try {
    await store.refreshCalendar()
  } finally {
    refreshingCalendar.value = false
  }
}
const overloadMin = computed(() => (capacity.value ? capacity.value.need_min - capacity.value.free_min : 0))
const capacityPct = computed(() => {
  const c = capacity.value
  if (!c) return 0
  return c.free_min <= 0 ? 100 : Math.min(100, Math.round((c.need_min / c.free_min) * 100))
})

function bucketEstimate(items: { effective_estimate_min: number }[]): string {
  return formatEstimate(items.reduce((n, it) => n + it.effective_estimate_min, 0))
}

function projectName(id: string | null): string {
  if (!id) return t('focus.columns.noProject')
  return projects.projects.find((p) => p.id === id)?.name ?? id
}
function chipStyle(id: string | null) {
  if (!id) return {}
  const h = projectHue(id)
  return { backgroundColor: `hsl(${h} 70% 50%)` }
}

function weekdayLabel(ymd: string): string {
  const [y, m, d] = ymd.split('-').map(Number)
  const date = new Date(y, m - 1, d)
  return `${date.toLocaleDateString(locale.value, { weekday: 'short' })} ${String(d).padStart(2, '0')}/${String(m).padStart(2, '0')}`
}
/** Overload is only known for today, and only with Calendar data (BR-08). */
function dayOverloaded(date: string): boolean {
  return date === today.value && !!capacity.value && overloadMin.value > 0
}

// ── Detail panel ──
const openId = ref<string | null>(null)
const panelRef = ref<InstanceType<typeof TaskDetailPanel> | null>(null)
/** Show another todo in the panel; an unsaved edit in it is asked about first. */
async function openTodo(id: string) {
  if (id === openId.value) return
  if (!(await (panelRef.value?.confirmDiscard() ?? true))) return
  openId.value = id
}
const openItem = computed(() => (openId.value ? store.items.find((it) => it.todo.id === openId.value) ?? null : null))
// Deleted elsewhere → close.
watch(openItem, (it) => {
  if (!it && openId.value && store.loaded) openId.value = null
})

// ── A reminder click / "open this todo" → open + flash it ──
const flashId = ref<string | null>(null)
let flashTimer: ReturnType<typeof setTimeout> | null = null
/** False when the board does not hold it (a done todo): the caller shows it elsewhere. */
async function showTodo(id: string | null | undefined): Promise<boolean> {
  if (!id) return false
  if (!store.loaded) await store.load()
  if (!store.items.some((it) => it.todo.id === id)) return false
  await openTodo(id)
  // Make sure its bucket is visible in the time view, then flash the row.
  const it = store.items.find((x) => x.todo.id === id)
  if (it && view.value === 'time' && !openBuckets.value.has(it.bucket)) toggleBucket(it.bucket)
  flashId.value = id
  if (flashTimer) clearTimeout(flashTimer)
  flashTimer = setTimeout(() => (flashId.value = null), 2500)
  await nextTick()
  document.querySelector(`[data-focus-row="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  return true
}

// ── Keys (V3-39): the selected row, ↑ ↓ ← → Space E S Enter ──
const boardEl = ref<HTMLElement | null>(null)
const selectedId = ref<string | null>(null)
const rowEl = (id: string) => boardEl.value?.querySelector<HTMLElement>(`[data-focus-row="${CSS.escape(id)}"]`) ?? null

/** Ids on screen in reading order, a list per day in the week (a todo shown twice counts once). */
function boardColumns(): string[][] {
  const root = boardEl.value
  if (!root) return []
  const frames = view.value === 'week' ? [...root.querySelectorAll('[data-week-date]')] : [root]
  return frames.map((f) => [...new Set([...f.querySelectorAll<HTMLElement>('[data-focus-row]')].map((el) => el.dataset.focusRow as string))])
}

async function selectRow(id: string) {
  selectedId.value = id
  await nextTick()
  const el = rowEl(id)
  el?.focus({ preventScroll: true })
  el?.scrollIntoView({ block: 'nearest' })
}
/** Focus (Tab, a click) on a row selects it. */
function onFocusIn(e: FocusEvent) {
  const id = (e.target as HTMLElement | null)?.closest<HTMLElement>('[data-focus-row]')?.dataset.focusRow
  if (id) selectedId.value = id
}

async function editTodo(id: string) {
  await openTodo(id)
  if (openId.value !== id) return
  await nextTick()
  panelRef.value?.startEdit()
}

async function snoozeTodo(id: string) {
  rowEl(id)?.querySelector<HTMLElement>('[data-snooze-trigger] button')?.click()
  await nextTick()
  await nextTick()
  document.querySelector<HTMLElement>('.ui-popover button:not(:disabled)')?.focus()
}

const keyboard = useTaskKeyboard({
  columns: boardColumns,
  week: () => view.value === 'week',
  selectedId,
  todoOf: (id) => store.items.find((it) => it.todo.id === id)?.todo ?? null,
  select: selectRow,
  open: openTodo,
  edit: editTodo,
  snooze: snoozeTodo,
})

// ── Week: drag a card to another day (V3-39) ──
function dayLabel(ymd: string): string {
  const [y, m, d] = ymd.split('-').map(Number)
  return new Date(y, m - 1, d).toLocaleDateString(locale.value, { weekday: 'long', day: 'numeric', month: 'numeric' })
}
async function moveToDay(id: string, date: string) {
  const todo = store.items.find((it) => it.todo.id === id)?.todo
  const patch = todo ? moveDueDate(todo, date) : null
  if (!patch) return
  try {
    await store.update(id, patch)
    toast.success(t('focus.week.movedTo', { date: dayLabel(date) }))
  } catch (e) {
    toast.error(String(e))
  }
}
const weekDrag = useWeekDrag({
  canDrag: (id) => store.items.some((it) => it.todo.id === id && !it.todo.done),
  onDrop: moveToDay,
})

onBeforeUnmount(() => {
  if (flashTimer) clearTimeout(flashTimer)
})

defineExpose({ showTodo, openTodo })
</script>

<template>
  <div class="relative flex h-full min-h-0">
  <div ref="boardEl" class="flex min-w-0 flex-1 flex-col" @keydown="keyboard.onKeydown" @focusin="onFocusIn">
    <!-- Today's load (BR-08) — only once the backend has Calendar data -->
    <div v-if="capacity" class="flex shrink-0 items-center gap-3 border-b border-border/60 px-6 py-2 text-xs">
      <span class="shrink-0">{{ t('focus.capacity.summary', { need: formatEstimate(capacity.need_min), free: formatEstimate(Math.max(0, capacity.free_min)) }) }}</span>
      <div class="h-1.5 min-w-24 flex-1 overflow-hidden rounded-full bg-muted">
        <div class="h-full rounded-full" :class="overloadMin > 0 ? 'bg-destructive' : 'bg-success'" :style="{ width: `${capacityPct}%` }" />
      </div>
      <span v-if="overloadMin > 0" class="flex shrink-0 items-center gap-1 font-medium text-destructive">
        <TriangleAlert class="h-3.5 w-3.5" :stroke-width="1.75" />
        {{ t('focus.capacity.overloaded', { over: formatEstimate(overloadMin) }) }}
      </span>
      <span v-if="capacity.busy.length" class="shrink-0 text-muted-foreground" :title="capacity.busy.map((b) => b.title).join('\n')">
        {{ t('focus.capacity.busy', { n: capacity.busy.length }, capacity.busy.length) }}
      </span>
      <span v-if="calendarOff" class="flex shrink-0 items-center gap-1 text-muted-foreground">
        <CalendarClock class="h-3.5 w-3.5" :stroke-width="1.75" />
        {{ t('focus.capacity.calendarOff') }}
      </span>
      <Button
        v-else
        variant="ghost"
        size="icon-sm"
        :disabled="refreshingCalendar"
        :title="t('focus.capacity.refresh')"
        :aria-label="t('focus.capacity.refresh')"
        @click="refreshCalendar"
      >
        <RefreshCw class="h-3.5 w-3.5" :class="refreshingCalendar ? 'animate-spin' : ''" :stroke-width="1.75" />
      </Button>
    </div>
    <div
      v-else-if="capacityError && capacityError !== CAPACITY_LOADING"
      class="flex shrink-0 items-center gap-2 border-b border-border/60 px-6 py-1.5 text-2xs text-muted-foreground"
    >
      <CalendarClock class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      <span v-if="capacityError === CAPACITY_NO_ACCOUNT" class="truncate">{{ t('focus.capacity.connect') }}</span>
      <span v-else class="truncate" :title="capacityError">{{ t('focus.capacity.error') }}</span>
      <Button
        v-if="capacityError !== CAPACITY_NO_ACCOUNT"
        variant="ghost"
        size="icon-sm"
        :disabled="refreshingCalendar"
        :title="t('focus.capacity.refresh')"
        :aria-label="t('focus.capacity.refresh')"
        @click="refreshCalendar"
      >
        <RefreshCw class="h-3.5 w-3.5" :class="refreshingCalendar ? 'animate-spin' : ''" :stroke-width="1.75" />
      </Button>
    </div>

    <!-- Toolbar: the three ways to read the board -->
    <div class="flex shrink-0 flex-wrap items-center gap-3 border-b border-border/60 px-6 py-2">
      <SegmentedControl
        v-model="view"
        :options="viewOptions"
        :stretch="false"
        :aria-label="t('focus.views.label')"
      />
    </div>

    <!-- First load -->
    <div v-if="!store.loaded && store.loading" class="space-y-3 p-6">
      <Skeleton class="h-24 w-full rounded-lg" />
      <Skeleton v-for="i in 6" :key="i" class="h-8 w-full rounded" />
    </div>

    <!-- Error before anything loaded -->
    <div v-else-if="store.error && !store.loaded" class="p-6">
      <Banner tone="destructive" class="items-center px-4 py-3 text-sm">
        {{ t('focus.loadError') }}: {{ store.error }}
        <template #actions>
          <Button variant="outline" size="sm" @click="store.load()">{{ t('focus.retry') }}</Button>
        </template>
      </Banner>
    </div>

    <!-- Empty -->
    <div v-else-if="store.items.length === 0" class="flex min-h-80 flex-1 flex-col items-center justify-center text-center">
      <div class="mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-muted">
        <Target class="h-6 w-6 text-muted-foreground" :stroke-width="1.5" />
      </div>
      <p class="text-sm font-medium">{{ t('focus.empty.title') }}</p>
      <p class="mt-1 max-w-96 text-xs text-muted-foreground">{{ t('focus.empty.hint') }}</p>
      <div class="mt-4"><AddTodoButton button-variant="primary" size="sm" /></div>
    </div>

    <p v-else-if="visible.length === 0" class="py-12 text-center text-sm text-muted-foreground">{{ t('focus.empty.filtered') }}</p>

    <!-- By time -->
    <div v-else-if="view === 'time'" class="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4">
      <Card v-if="top.length" class="border-primary/30" body-class="p-2">
        <h2 class="flex items-center gap-1.5 px-2 pb-1 pt-0.5 text-2xs font-semibold uppercase tracking-wide text-primary">
          <Target class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('focus.nowSection') }}
        </h2>
        <FocusTodoRow
          :data-focus-row="it.todo.id"
          :class="flashId === it.todo.id ? 'ring-2 ring-primary/70' : ''"
          v-for="(it, i) in top"
          :key="it.todo.id"
          :item="it"
          :now="now"
          :rank="i + 1"
          :selected="selectedId === it.todo.id"
          @open="openTodo($event)"
        />
      </Card>

      <section v-for="g in buckets" :key="g.bucket">
        <button
          type="button"
          class="flex w-full cursor-pointer items-center gap-1.5 rounded px-1 py-1 text-left text-xs font-semibold transition-colors hover:bg-accent/40"
          :class="g.bucket === 'overdue' ? 'text-destructive' : 'text-foreground'"
          :aria-expanded="openBuckets.has(g.bucket)"
          @click="toggleBucket(g.bucket)"
        >
          <component :is="openBuckets.has(g.bucket) ? ChevronDown : ChevronRight" class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
          {{ t(`focus.buckets.${g.bucket}`) }}
          <span class="font-normal text-muted-foreground">({{ g.items.length }})</span>
          <span v-if="isActiveBucket(g.bucket)" class="ml-auto font-normal text-muted-foreground tabular-nums">~{{ bucketEstimate(g.items) }}</span>
        </button>
        <div v-if="openBuckets.has(g.bucket)" class="mt-0.5">
          <FocusTodoRow
          :data-focus-row="it.todo.id"
          :class="flashId === it.todo.id ? 'ring-2 ring-primary/70' : ''" v-for="it in g.items" :key="it.todo.id" :item="it" :now="now" :selected="selectedId === it.todo.id" @open="openTodo($event)" />
        </div>
      </section>
    </div>

    <!-- By project -->
    <div v-else-if="view === 'project'" class="flex min-h-0 flex-1 gap-3 overflow-x-auto px-6 py-4">
      <div
        v-for="col in projectColumns"
        :key="col.projectId ?? '__none'"
        class="flex w-72 shrink-0 flex-col rounded-lg border border-border/60 bg-muted/20"
      >
        <div class="flex shrink-0 items-center gap-2 border-b border-border/60 px-3 py-2">
          <span v-if="col.projectId" class="h-2.5 w-2.5 shrink-0 rounded-full" :style="chipStyle(col.projectId)" />
          <span class="min-w-0 flex-1 truncate text-xs font-semibold">{{ projectName(col.projectId) }}</span>
          <span class="shrink-0 text-2xs tabular-nums text-muted-foreground">~{{ formatEstimate(col.estimateMin) }}</span>
          <span v-if="col.overdue" class="shrink-0 text-2xs font-medium text-destructive">
            {{ t('focus.columns.overdue', { n: col.overdue }) }}
          </span>
        </div>
        <div class="min-h-0 flex-1 space-y-1.5 overflow-y-auto p-2">
          <FocusTodoRow
          :data-focus-row="it.todo.id"
          :class="flashId === it.todo.id ? 'ring-2 ring-primary/70' : ''"
            v-for="it in col.items"
            :key="it.todo.id"
            :item="it"
            :now="now"
            layout="card"
            hide-project
            :selected="selectedId === it.todo.id"
            @open="openTodo($event)"
          />
        </div>
      </div>
    </div>

    <!-- Week -->
    <div v-else class="min-h-0 flex-1 overflow-x-auto px-6 py-4">
      <div class="grid h-full min-w-[56rem] grid-cols-7 gap-2">
        <div
          v-for="col in weekColumns"
          :key="col.date"
          :data-week-date="col.date"
          class="flex min-w-0 flex-col rounded-lg border"
          :class="[
            weekDrag.overDate.value === col.date && weekDrag.dragId.value
              ? 'border-primary bg-primary/5 ring-1 ring-primary'
              : dayOverloaded(col.date) ? 'border-destructive/50 bg-destructive/5' : 'border-border/60 bg-muted/20',
            col.date === today ? 'ring-1 ring-primary/40' : '',
          ]"
        >
          <div class="flex shrink-0 items-center gap-1 border-b border-border/60 px-2 py-1.5">
            <span class="flex-1 truncate text-2xs font-semibold" :class="col.date === today ? 'text-primary' : ''">{{ weekdayLabel(col.date) }}</span>
            <span v-if="col.estimateMin" class="text-2xs tabular-nums text-muted-foreground" :title="t('focus.week.estimateHint')">
              ~{{ formatEstimate(col.estimateMin) }}
            </span>
          </div>
          <p v-if="dayOverloaded(col.date)" class="px-2 pt-1 text-2xs font-medium text-destructive">
            {{ t('focus.week.overloaded') }} · {{ span(overloadMin) }}
          </p>
          <div class="min-h-0 flex-1 space-y-1.5 overflow-y-auto p-1.5">
            <FocusTodoRow
              v-for="it in col.items"
              :key="it.todo.id"
              :data-focus-row="it.todo.id"
              :class="flashId === it.todo.id ? 'ring-2 ring-primary/70' : ''"
              :style="weekDrag.cardStyle(it.todo.id)"
              :item="it"
              :now="now"
              layout="card"
              :selected="selectedId === it.todo.id"
              @pointerdown="weekDrag.start(it.todo.id, $event)"
              @click.capture="weekDrag.swallowDragClick"
              @open="openTodo($event)"
            />
          </div>
        </div>
      </div>
    </div>

    <p
      v-if="store.loaded && visible.length > 0"
      class="shrink-0 border-t border-border/60 px-6 py-1.5 text-2xs text-muted-foreground"
    >
      {{ t('focus.keysHint') }}<template v-if="view === 'week'"> · {{ t('focus.week.dragHint') }}</template>
    </p>
  </div>

  <!-- The todo's panel: beside the board (it stays visible and clickable); under 900px it floats over it. -->
  <aside
    v-if="openItem"
    class="w-[440px] shrink-0 border-l border-border bg-background min-[1920px]:w-[520px] max-[901px]:absolute max-[901px]:inset-y-0 max-[901px]:right-0 max-[901px]:z-20 max-[901px]:max-w-full max-[901px]:shadow-xl"
  >
    <TaskDetailPanel ref="panelRef" :todo="openItem.todo" mode="dock" @close="openId = null" @deleted="openId = null" />
  </aside>
  <FocusDialogs />
  <FocusAiDialogs />
  </div>
</template>
