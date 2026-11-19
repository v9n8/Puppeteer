<script setup lang="ts">
// Board Session (V3-22): one column per flag, a card per Session; dragging a card to
// another column sets its flag (the same `set_run_flag` as the menus). One Project
// or all of them. The data and the drop live in stores/flagBoard; this file is the
// layout, the keyboard and the status line.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { Check, ChevronDown, Folder, RefreshCw, Search, Settings2 } from 'lucide-vue-next'
import { Banner, Button, ContextMenu, DropdownItem, DropdownMenu, DropdownSeparator, PageHeader, Skeleton } from '@/components/ui'
import RunActionItems from '@/components/RunActionItems.vue'
import BoardColumn from '@/components/board/BoardColumn.vue'
import BoardCard from '@/components/board/BoardCard.vue'
import { useBoardDrag } from '@/components/board/useBoardDrag'
import { useBoardRunActions } from '@/components/board/useBoardRunActions'
import { useFlagTargets } from '@/components/sessions/useFlagTargets'
import { useFlagBoardStore, type BoardRun } from '@/stores/flagBoard'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useProjectsStore } from '@/stores/projects'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import { useUILayoutStore } from '@/stores/uiLayout'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { useToast } from '@/composables/useToast'
import { flagById, flagLabel } from '@/lib/sessionFlags'
import { sessionNavTarget } from '@/lib/sidebarNav'
import {
  boardColumns, canDrop, cardTitle, dropFlagId, filterCards, groupCards, moveSelection, resolveBoardProject,
  NO_FLAG_COLUMN, type BoardColumn as Column, type BoardKeyState,
} from '@/lib/flagBoard'

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
const { toast } = useToast()
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
const columns = computed(() => boardColumns(flags.value, mode.value))
const runById = computed(() => new Map((board.data?.runs ?? []).map((r) => [r.id, r])))

function columnLabel(c: Column): string {
  if (c.kind === 'none') return t('board.column.none')
  if (c.kind === 'own') return t('board.column.own')
  return c.flag ? flagLabel(c.flag, t) : ''
}

const grouped = computed(() => groupCards(board.data?.runs ?? [], columns.value))
const shown = computed(() => filterCards(grouped.value, query.value, (r) => cardTitle(r, t)))
const columnOfRun = (run: BoardRun): string => {
  for (const [id, cards] of grouped.value) if (cards.includes(run)) return id
  return NO_FLAG_COLUMN
}

// "No flag" starts folded on the All projects board; the user's choice is kept per machine.
const noFlagCollapsed = computed(() => ui.boardNoFlagCollapsed ?? mode.value === 'all')
const isCollapsed = (c: Column) => c.kind === 'none' && noFlagCollapsed.value

const hasRuns = computed(() => (board.data?.runs.length ?? 0) > 0)
const noFlagYet = computed(() => !!board.data && hasRuns.value && board.data.runs.every((r) => !r.flag_id))

function ownFlagOf(run: BoardRun) {
  return mode.value === 'all' && run.flag_id ? flagById(flags.value, run.flag_id) ?? null : null
}

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

// ---- Drop ---------------------------------------------------------------------------

async function dropCard(run: BoardRun, columnId: string) {
  const column = columns.value.find((c) => c.id === columnId)
  if (!column || !canDrop(run, column, hidden.value)) return
  const flagId = dropFlagId(column)
  if ((run.flag_id ?? null) === flagId) return
  const name = cardTitle(run, t)
  try {
    const before = await board.setFlag(run, flagId)
    const message = column.flag
      ? t('board.toast.set', { flag: columnLabel(column), name })
      : t('board.toast.removed', { name })
    toast.success(message, 6000, { label: t('board.toast.undo'), run: () => void undo(run, before, name) })
  } catch (e) {
    toast.error(String(e))
  }
}

async function undo(run: BoardRun, before: string | null, name: string) {
  try {
    await board.setFlag(run, before)
    toast.success(t('board.toast.undone', { name }))
  } catch (e) {
    toast.error(String(e))
  }
}

const boardEl = ref<HTMLElement | null>(null)
const { drag, onCardPointerDown, consumeClick } = useBoardDrag({
  board: boardEl,
  onDrop: (run, id) => void dropCard(run, id),
  canLand: (run, id) => {
    const column = columns.value.find((c) => c.id === id)
    return !!column && canDrop(run, column, hidden.value)
  },
  columnOf: columnOfRun,
})

/** How the dragged / lifted card sees a column. */
function dropState(c: Column): 'idle' | 'target' | 'blocked' {
  const lifted = keys.value.lifted
  if (lifted) {
    const run = runById.value.get(lifted.runId)
    return run && lifted.target === c.id && lifted.target !== lifted.from ? (canDrop(run, c, hidden.value) ? 'target' : 'blocked') : 'idle'
  }
  const d = drag.value
  if (!d || d.over !== c.id || c.id === columnOfRun(d.run)) return 'idle'
  return canDrop(d.run, c, hidden.value) ? 'target' : 'blocked'
}

function blockedReason(c: Column): string {
  const run = drag.value?.run
  if (c.kind === 'own') return t('board.column.ownLock')
  return run ? t('board.column.blocked', { project: run.project_name }) : ''
}

/** The dragged card came from the "own flags" column, so its copy shows that flag too. */
const dragFromOwn = computed(() => {
  const d = drag.value
  return !!d && columns.value.find((c) => c.id === columnOfRun(d.run))?.kind === 'own'
})

function cardState(run: BoardRun): 'idle' | 'lifted' | 'dragging' {
  if (drag.value?.run.id === run.id) return 'dragging'
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
  else if (effect?.kind === 'drop') void dropCard(run, effect.columnId)
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
  if (drag.value) return `${t('board.drag.mouse', { name: cardTitle(drag.value.run, t) })} ${t('board.drag.keys')}`
  return t('board.drag.idle')
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
      class="min-h-0 flex-1 overflow-x-auto overflow-y-hidden px-4 pb-4 pt-3"
      :class="{ 'select-none': drag }"
      @keydown="onBoardKeydown"
      @focusout="onBoardFocusOut"
    >
      <div v-if="board.data" class="flex h-full min-w-min gap-3" :class="{ 'cursor-grabbing': drag }">
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
          @toggle="ui.setBoardNoFlagCollapsed(!noFlagCollapsed)"
        >
          <template #card="{ card }">
            <BoardCard
              :run="card"
              :waiting="!!live.get(card.id)?.permissionQueue[0]"
              :saving="board.saving.has(card.id)"
              :show-project="mode === 'all'"
              :own-flag="c.kind === 'own' ? ownFlagOf(card) : null"
              :state="cardState(card)"
              @press="onCardPointerDown($event, card)"
              @menu="(x, y) => openMenu(card, x, y)"
              @open="onCardClick(card)"
            />
          </template>
        </BoardColumn>
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
          :own-flag="dragFromOwn ? ownFlagOf(drag.run) : null"
          state="lifted"
        />
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
