<script setup lang="ts">
// Board Session (V3-22): one column per flag, a card per Session; dragging a card to
// another column sets its flag (the same `set_run_flag` as the menus). One Project
// or all of them; a box on empty space marks cards that then drag as a group. The data
// lives in stores/flagBoard, the marks and the drop in components/board; this file is
// the layout, the keyboard and the status line.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { Check, ChevronDown, Columns3, Folder, ListFilter, RefreshCw, Search, Settings2 } from 'lucide-vue-next'
import { Banner, Button, ContextMenu, DropdownItem, DropdownMenu, DropdownSeparator, PageHeader, Skeleton } from '@/components/ui'
import RunActionItems from '@/components/RunActionItems.vue'
import BoardColumn from '@/components/board/BoardColumn.vue'
import BoardCard from '@/components/board/BoardCard.vue'
import BoardFilterPopover from '@/components/board/BoardFilterPopover.vue'
import { useBoardDrag } from '@/components/board/useBoardDrag'
import { useBoardMarks } from '@/components/board/useBoardMarks'
import { useBoardGroupDrop } from '@/components/board/useBoardGroupDrop'
import { useBoardRunActions } from '@/components/board/useBoardRunActions'
import { useBoardFilter } from '@/components/board/useBoardFilter'
import { useFlagTargets } from '@/components/sessions/useFlagTargets'
import { useFlagBoardStore, type BoardRun } from '@/stores/flagBoard'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useProjectsStore } from '@/stores/projects'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import { useUILayoutStore } from '@/stores/uiLayout'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { flagLabel } from '@/lib/sessionFlags'
import { sessionNavTarget } from '@/lib/sidebarNav'
import {
  canDrop, cardTitle, filterCards, groupCards, moveSelection, resolveBoardProject, type BoardColumn as Column, type BoardKeyState,
} from '@/lib/flagBoard'
import { hiddenCount } from '@/lib/boardFilter'

defineOptions({ name: 'BoardView' })

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const board = useFlagBoardStore()
const ui = useUILayoutStore()
const projectStore = useProjectsStore()
const workspaceTabs = useWorkspaceTabsStore()
const live = useLiveRunsStore()
const sessionFlags = useSessionFlagsStore()
const { canManage, canOpenProjectFlags, openManage, openProjectFlags } = useFlagTargets()

// ---- Which Project -----------------------------------------------------------------

const projectIds = computed(() => projectStore.projects.map((p) => p.id))
const tabProject = computed(() => sessionNavTarget(workspaceTabs.tabs, workspaceTabs.recent, null)?.projectId ?? null)
const choice = computed(() => resolveBoardProject(ui.boardProject, projectIds.value, tabProject.value))
const mode = computed(() => (choice.value === 'all' ? 'all' : 'project'))
const scope = computed(() => (choice.value === 'all' ? null : choice.value))
const choiceName = computed(() =>
  choice.value === 'all' ? t('board.project.all') : (projectStore.projects.find((p) => p.id === choice.value)?.name ?? t('board.project.choose')),
)
const projectName = computed(() => (choice.value === 'all' ? '' : choiceName.value))

function chooseProject(id: string) {
  ui.setBoardProject(id)
}

// ---- Columns and cards -------------------------------------------------------------

const query = ref('')
const flags = computed(() => board.data?.flags ?? [])
const hidden = computed(() => board.data?.hidden ?? [])
// Which columns show, and in what order: the board filter of this scope (UR-60).
const { filter, columns, isDefault, apply: applyFilter } = useBoardFilter({ scope: choice, flags, projectOrder: projectIds })
const filterOpen = ref(false)
const runById = computed(() => new Map((board.data?.runs ?? []).map((r) => [r.id, r])))

function columnLabel(c: Column): string {
  if (c.kind === 'none') return t('board.column.none')
  return c.flag ? flagLabel(c.flag) : ''
}

const grouped = computed(() => groupCards(board.data?.runs ?? [], columns.value))
const shown = computed(() => filterCards(grouped.value, query.value, (r) => cardTitle(r, t)))

// "No flag" starts folded on the All projects board; the user's choice is kept per machine.
const noFlagCollapsed = computed(() => ui.boardNoFlagCollapsed ?? mode.value === 'all')
const isCollapsed = (c: Column) => c.kind === 'none' && noFlagCollapsed.value

const hasRuns = computed(() => (board.data?.runs.length ?? 0) > 0)
const noFlagYet = computed(() => !!board.data && hasRuns.value && board.data.runs.every((r) => !r.flag_id))

const projectNameOf = (id: string) => projectStore.projects.find((p) => p.id === id)?.name ?? ''
/** The All projects board names the Project under the head of an own flag's column. */
const columnProject = (c: Column) => (mode.value === 'all' && c.flag?.project_id ? projectNameOf(c.flag.project_id) : '')

// ---- Loading -----------------------------------------------------------------------

watch(scope, (id) => { void board.load(id) })
onMounted(() => {
  void projectStore.fetchProjects().then(() => void board.load(scope.value))
  void board.start()
  void sessionFlags.load(null)
  applyRouteProject()
})
onBeforeUnmount(() => board.stop())

// "View as board" (History's flag filter) arrives with the Project in the URL.
function applyRouteProject() {
  const asked = route.query.project
  if (typeof asked !== 'string' || !asked) return
  ui.setBoardProject(asked)
  router.replace({ path: route.path }).catch(() => {})
}
watch(() => route.query.project, applyRouteProject)

// ---- Drop and marks -----------------------------------------------------------------

const boardEl = ref<HTMLElement | null>(null)
/** The cards on screen in display order: columns left to right, cards top to bottom; a folded "No flag" shows none. */
const visibleCards = computed(() => columns.value.filter((c) => !isCollapsed(c)).flatMap((c) => shown.value.get(c.id)?.cards ?? []))
const { marks, box, onPointerDown: onBoardPointerDown, groupOf } = useBoardMarks({
  scroller: boardEl,
  visible: visibleCards,
  choice,
  busy: () => !!drag.value || !!keys.value.lifted || filterOpen.value,
  menuOpen: () => !!menu.value,
})
const { overPlan, blockReasonFor, dropGroup } = useBoardGroupDrop({ columns, hidden, drag: () => drag.value, columnLabel, projectNameOf })

const { drag, onCardPointerDown, consumeClick } = useBoardDrag({
  board: boardEl,
  group: groupOf,
  onDrop: (runs, id) => {
    // A group released on a column is done with, whatever could land there.
    if (runs.some((r) => marks.value.has(r.id))) marks.value = new Set()
    void dropGroup(runs, id)
  },
})
const draggedIds = computed(() => new Set(drag.value?.runs.map((r) => r.id) ?? []))

/** How the dragged / lifted card sees a column; a group shows "target" while one of its cards can land. */
function dropState(c: Column): 'idle' | 'target' | 'blocked' {
  const lifted = keys.value.lifted
  if (lifted) {
    const run = runById.value.get(lifted.runId)
    return run && lifted.target === c.id && lifted.target !== lifted.from ? (canDrop(run, c, hidden.value) ? 'target' : 'blocked') : 'idle'
  }
  const plan = overPlan.value
  if (!plan || drag.value?.over !== c.id) return 'idle'
  return plan.move.length ? 'target' : plan.blocked.length ? 'blocked' : 'idle'
}

/** The lifted card's reason, or for a drag the first refused card's. */
function blockedReason(c: Column): string {
  const lifted = keys.value.lifted ? runById.value.get(keys.value.lifted.runId) : undefined
  if (lifted) return blockReasonFor(lifted, c)
  const first = drag.value?.over === c.id ? overPlan.value?.blocked[0] : undefined
  return first ? blockReasonFor(first, c) : ''
}

function cardState(run: BoardRun): 'idle' | 'lifted' | 'dragging' {
  if (draggedIds.value.has(run.id)) return 'dragging'
  return keys.value.lifted?.runId === run.id ? 'lifted' : 'idle'
}

// ---- Keyboard -----------------------------------------------------------------------

const keys = ref<BoardKeyState>({ cursor: null, lifted: null })

function onBoardKeydown(ev: KeyboardEvent) {
  const el = (ev.target as HTMLElement | null)?.closest<HTMLElement>('[data-card-id]')
  if (!el || ev.altKey || ev.ctrlKey || ev.metaKey) return
  const cursor = keys.value.lifted?.runId ?? el.dataset.cardId ?? null
  const run = cursor ? runById.value.get(cursor) : undefined
  if (!run) return
  const open = columns.value.filter((c) => !isCollapsed(c))
  const layout = {
    columnIds: open.map((c) => c.id),
    cardIds: Object.fromEntries(open.map((c) => [c.id, (shown.value.get(c.id)?.cards ?? []).map((r) => r.id)])),
    dropColumnIds: open.filter((c) => canDrop(run, c, hidden.value)).map((c) => c.id),
  }
  const input = { ...keys.value, cursor }
  const { state, effect } = moveSelection(layout, input, ev.key)
  // Not a board key (Tab, a letter), or Esc with nothing lifted (the open menu closes itself).
  if ((state === input && !effect) || effect?.kind === 'escape') return
  ev.preventDefault()
  const moved = state.cursor !== cursor
  keys.value = state
  if (effect?.kind === 'open') openSession(run)
  else if (effect?.kind === 'drop') void dropGroup([run], effect.columnId)
  if (moved && state.cursor) {
    // The card the cursor moved to takes the focus once the DOM is current.
    const id = state.cursor
    requestAnimationFrame(() => boardEl.value?.querySelector<HTMLElement>(`[data-card-id="${CSS.escape(id)}"]`)?.focus())
  }
}

function onBoardFocusOut(ev: FocusEvent) {
  if (keys.value.lifted && !boardEl.value?.contains(ev.relatedTarget as Node | null)) keys.value = { ...keys.value, lifted: null }
}

// ---- Card actions -------------------------------------------------------------------

const actions = useBoardRunActions(() => void board.reload())
const { menu, openMenu, closeMenu, openSession, actionProps, actionHandlers } = actions

function onCardClick(run: BoardRun) {
  if (!consumeClick()) openSession(run)
}

// ---- Status line --------------------------------------------------------------------

const statusLine = computed(() => {
  const lifted = keys.value.lifted
  if (lifted) {
    const run = runById.value.get(lifted.runId)
    const column = columns.value.find((c) => c.id === lifted.target)
    return run && column ? t('board.drag.lifted', { name: cardTitle(run, t), column: columnLabel(column) }) : ''
  }
  const d = drag.value
  if (d && d.runs.length > 1) {
    const group = t('board.drag.group', { n: d.runs.length })
    const k = overPlan.value?.blocked.length ?? 0
    return k ? `${group} ${t('board.drag.groupPartial', { k, n: d.runs.length })}` : group
  }
  if (d) {
    const over = d.over ? columns.value.find((c) => c.id === d.over) : undefined
    const name = cardTitle(d.run, t)
    if (over && dropState(over) === 'blocked') return t('board.drag.blocked', { name, reason: blockedReason(over) })
    return `${t('board.drag.mouse', { name })} ${t('board.drag.keys')}`
  }
  const n = board.data ? hiddenCount(board.data.runs, columns.value) : 0
  const base = marks.value.size ? t('board.drag.marked', { n: marks.value.size }) : t('board.drag.idle')
  return n ? `${t('board.filter.hidden', { n })} · ${base}` : base
})

const manageTarget = computed(() => (mode.value === 'all' ? (canManage ? 'shared' : null) : canOpenProjectFlags ? 'project' : null))
function manageFlags() {
  if (mode.value === 'all') openManage()
  else if (scope.value) openProjectFlags(scope.value)
}
</script>

<template>
  <div class="flex h-full min-w-0 flex-col">
    <PageHeader :title="t('board.title')">
      <template #meta>
      <DropdownMenu align="left" class="min-w-0">
        <template #trigger>
          <button
            type="button"
            class="inline-flex h-8 w-[150px] min-w-0 cursor-pointer items-center gap-2 rounded-md border border-input bg-card pl-2.5 pr-2 text-sm text-foreground transition-colors hover:bg-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :title="t('board.project.tooltip', { name: choiceName })"
            :aria-label="t('board.project.tooltip', { name: choiceName })"
            aria-haspopup="menu"
          >
            <Folder class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <span class="min-w-0 truncate">{{ choiceName }}</span>
            <ChevronDown class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
          </button>
        </template>
        <div class="max-h-80 min-w-[14rem] overflow-y-auto">
          <DropdownItem @click="chooseProject('all')">
            <span class="min-w-0 flex-1 truncate">{{ t('board.project.all') }}</span>
            <Check class="h-3.5 w-3.5 shrink-0 text-primary" :class="{ invisible: choice !== 'all' }" :stroke-width="2" />
          </DropdownItem>
          <DropdownSeparator />
          <DropdownItem v-for="p in projectStore.projects" :key="p.id" @click="chooseProject(p.id)">
            <span class="min-w-0 flex-1 truncate">{{ p.name }}</span>
            <Check class="h-3.5 w-3.5 shrink-0 text-primary" :class="{ invisible: choice !== p.id }" :stroke-width="2" />
          </DropdownItem>
        </div>
      </DropdownMenu>

      <BoardFilterPopover
        v-model:open="filterOpen"
        :filter="filter"
        :flags="flags"
        :projects="projectStore.projects"
        :badge="isDefault ? null : columns.length"
        @apply="applyFilter"
      />

      <label class="flex h-8 w-[220px] shrink items-center gap-1.5 rounded-md border border-input bg-card px-2.5 text-sm transition-colors focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/15 min-[901px]:min-w-[220px] max-[901px]:w-[150px]">
        <Search class="pointer-events-none h-4 w-4 shrink-0 text-muted-foreground" :stroke-width="1.75" />
        <input
          v-model="query"
          type="text"
          class="min-w-0 flex-1 bg-transparent text-sm text-foreground placeholder:text-muted-foreground focus:outline-none"
          :placeholder="t('board.filter.placeholder')"
          :aria-label="t('board.filter.label')"
        />
      </label>
      </template>

      <template #actions>
        <Button variant="ghost" size="icon-xs" :disabled="board.loading" :title="t('board.refresh')" :aria-label="t('board.refresh')" @click="board.reload()">
          <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': board.loading }" :stroke-width="1.75" />
        </Button>
        <Button
          v-if="manageTarget"
          variant="ghost"
          size="icon-sm"
          :title="t('board.manageFlagsTip')"
          :aria-label="t('board.manageFlags')"
          @click="manageFlags"
        >
          <Settings2 class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
      </template>
    </PageHeader>

    <div v-if="board.error" class="shrink-0 px-4 pt-3">
      <Banner tone="destructive" live>
        {{ t('board.banner.error', { error: board.error }) }}
        <template #actions>
          <Button variant="outline" size="sm" @click="board.reload()">{{ t('board.banner.retry') }}</Button>
        </template>
      </Banner>
    </div>
    <div v-else-if="noFlagYet" class="shrink-0 px-4 pt-3">
      <Banner tone="info">
        {{ projectName ? t('board.banner.noFlags', { project: projectName }) : t('board.banner.noFlagsAll') }}
      </Banner>
    </div>

    <!-- The board scrolls sideways; every column scrolls its cards by itself. -->
    <div
      ref="boardEl"
      class="relative min-h-0 flex-1 overflow-x-auto overflow-y-hidden px-4 pb-4 pt-3"
      :class="{ 'select-none': drag || box }"
      @pointerdown="onBoardPointerDown"
      @keydown="onBoardKeydown"
      @focusout="onBoardFocusOut"
    >
      <div v-if="board.data && columns.length" class="flex h-full min-w-min gap-3" :class="{ 'cursor-grabbing': drag }">
        <BoardColumn
          v-for="c in columns"
          :key="c.id"
          :column="c"
          :label="columnLabel(c)"
          :cards="shown.get(c.id)?.cards ?? []"
          :matched="shown.get(c.id)?.matched ?? 0"
          :total="shown.get(c.id)?.total ?? 0"
          :filtering="query.trim() !== ''"
          :query="query"
          :collapsed="isCollapsed(c)"
          :drop="dropState(c)"
          :blocked-reason="blockedReason(c)"
          :project="columnProject(c)"
          @toggle="ui.setBoardNoFlagCollapsed(!noFlagCollapsed)"
        >
          <template #card="{ card }">
            <BoardCard
              :run="card"
              :waiting="!!live.get(card.id)?.permissionQueue[0]"
              :saving="board.saving.has(card.id)"
              :show-project="mode === 'all'"
              :state="cardState(card)"
              :marked="marks.has(card.id)"
              @press="onCardPointerDown($event, card)"
              @menu="(x, y) => openMenu(card, x, y)"
              @open="onCardClick(card)"
            />
          </template>
        </BoardColumn>
      </div>
      <!-- The filter has every column off. -->
      <div v-else-if="board.data" data-board-empty class="flex h-full flex-col items-center justify-center gap-3 text-center">
        <Columns3 class="h-8 w-8 text-muted-foreground" :stroke-width="1.25" />
        <p class="text-sm font-medium">{{ t('board.empty.title') }}</p>
        <p class="max-w-[320px] text-sm text-muted-foreground">{{ t('board.empty.body') }}</p>
        <Button variant="outline" class="px-3 text-sm font-normal" @click="filterOpen = true">
          <ListFilter class="h-3.5 w-3.5" :stroke-width="1.75" />{{ t('board.empty.open') }}
        </Button>
      </div>
      <!-- Not loaded yet (or it failed): empty frames, the count is a dash. -->
      <div v-else class="flex h-full min-w-min gap-3" :aria-busy="board.loading">
        <div v-for="n in 4" :key="n" class="flex min-w-[12.25rem] max-w-[20rem] flex-1 basis-[12.25rem] flex-col gap-2 rounded-lg border border-border bg-muted/30 p-2.5">
          <div class="flex items-center justify-between"><Skeleton class="h-4 w-24" /><span class="font-mono text-2xs text-muted-foreground">{{ board.loading ? '' : '—' }}</span></div>
          <template v-if="board.loading">
            <Skeleton class="h-14 w-full" />
            <Skeleton class="h-14 w-full" />
          </template>
        </div>
      </div>
      <!-- The selection box, over the columns. -->
      <div
        v-if="box"
        data-board-box
        class="pointer-events-none absolute z-[1] rounded-sm border border-primary bg-primary/10"
        :style="{ left: `${box.left}px`, top: `${box.top}px`, width: `${box.right - box.left}px`, height: `${box.bottom - box.top}px` }"
        aria-hidden="true"
      />
    </div>

    <!-- The card being dragged, following the pointer from where it was grabbed. -->
    <Teleport to="body">
      <div
        v-if="drag && drag.width"
        class="pointer-events-none fixed left-0 top-0 z-[90] rotate-[1.5deg] shadow-lg shadow-black/20"
        :style="{ width: `${drag.width}px`, translate: `${drag.x - drag.grabX}px ${drag.y - drag.grabY}px` }"
        aria-hidden="true"
      >
        <BoardCard
          :run="drag.run"
          :waiting="!!live.get(drag.run.id)?.permissionQueue[0]"
          :saving="false"
          :show-project="mode === 'all'"
          state="lifted"
        />
        <!-- A group: how many cards travel with it. -->
        <span
          v-if="drag.runs.length > 1"
          class="absolute -right-2 -top-2 rounded-full bg-primary px-1.5 py-0.5 font-mono text-2xs font-semibold text-primary-foreground shadow"
        >×{{ drag.runs.length }}</span>
      </div>
    </Teleport>

    <p class="h-8 shrink-0 truncate border-t border-border px-4 text-sm leading-8 text-muted-foreground" role="status" aria-live="polite">
      {{ statusLine }}
    </p>

    <ContextMenu :open="!!menu" :x="menu?.x ?? 0" :y="menu?.y ?? 0" @close="closeMenu">
      <template v-if="menu">
        <RunActionItems v-bind="actionProps(menu.run)" section="actions" v-on="actionHandlers(menu.run)" />
        <RunActionItems v-bind="actionProps(menu.run)" section="delete" v-on="actionHandlers(menu.run)" />
      </template>
    </ContextMenu>
  </div>
</template>
