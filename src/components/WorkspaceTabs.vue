<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { Ban, Check, ChevronDown, Eye, FolderOpen, Plus, SlidersHorizontal, X } from 'lucide-vue-next'
import { Button, ContextMenu, DropdownItem, DropdownMenu, DropdownSeparator } from '@/components/ui'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useProjectsStore } from '@/stores/projects'
import { useDragSort } from '@/composables/useDragSort'
import { useProjectSwitcher } from '@/composables/useProjectSwitcher'
import { useToast } from '@/composables/useToast'
import {
  PROJECT_COLORS, projectColor, projectIcon, projectTextClass, swatchClass, type ProjectColor,
} from '@/lib/projectAppearance'
import { tabsToClose, type TabCloseScope } from '@/lib/projectSwitcher'

/**
 * Horizontal tab bar for fast switching between PROJECTS. One tab per open
 * project; switching runs inside a project is still done from RunView's History
 * list. Clicking a tab returns to the project's last-viewed run. Aggregate live
 * status (any run streaming / awaiting permission) is read from liveRuns.
 *
 * The only place that opens a project's sessions (D3), so its right-click menu
 * (UX-51) carries what you'd do to a project from here: new session, close
 * tabs, view it, recolour it, reveal it in Finder.
 */

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const { toast } = useToast()
const tabsStore = useWorkspaceTabsStore()
const live = useLiveRunsStore()
const projectsStore = useProjectsStore()
const { newSession } = useProjectSwitcher()

const activeProjectId = computed(() =>
  typeof route.params.projectId === 'string' ? route.params.projectId : null,
)

function projectOf(projectId: string) {
  return projectsStore.projects.find((p) => p.id === projectId)
}

function projectName(projectId: string): string {
  return projectOf(projectId)?.name ?? t('misc.tabs.project')
}

/**
 * Each open project's icon (UX-08) and colour: the icon tinted with it, and the
 * status dot filled with it (UX-51) — a 13px tinted outline alone was barely
 * visible. A computed for the same reason as `states` below: a drag re-renders
 * the strip on every pointer frame.
 */
const tabLooks = computed(() => {
  const map: Record<string, { icon: Component; tint: string; swatch: string }> = {}
  for (const tab of tabsStore.tabs) {
    const p = projectOf(tab.projectId)
    const color = projectColor(p?.color)
    map[tab.projectId] = {
      icon: projectIcon(p?.icon),
      tint: projectTextClass(p?.color),
      swatch: color ? swatchClass(color) : '',
    }
  }
  return map
})

interface ProjectState {
  running: boolean
  pending: number
}

/**
 * Live status per open project, built in one pass over the sessions. A computed
 * rather than a per-tab helper so a drag — which re-renders the strip on every
 * pointer frame — doesn't re-scan every session four times per tab per frame.
 */
const states = computed(() => {
  const map: Record<string, ProjectState> = {}
  for (const tab of tabsStore.tabs) map[tab.projectId] = { running: false, pending: 0 }
  live.sessions.forEach((s) => {
    const st = map[s.projectId]
    if (!st) return
    if (s.status === 'running') st.running = true
    st.pending += s.permissionQueue.length
  })
  return map
})

function stateOf(projectId: string): ProjectState {
  return states.value[projectId] ?? { running: false, pending: 0 }
}

/**
 * The dot's fill: amber while a permission waits (it needs the user), else the
 * project's colour, else the old running / idle tones. Running is also shown by
 * the ping around the dot, so a coloured project loses nothing.
 */
function dotClass(projectId: string): string {
  if (stateOf(projectId).pending > 0) return 'bg-warning'
  return tabLooks.value[projectId]?.swatch || (stateOf(projectId).running ? 'bg-primary' : 'bg-muted-foreground/40')
}

function select(projectId: string) {
  if (projectId === activeProjectId.value) return
  const tab = tabsStore.tabs.find((t) => t.projectId === projectId)
  if (tab?.lastRunId) {
    router
      .push({ name: 'project-run-detail', params: { projectId, runId: tab.lastRunId } })
      .catch(() => {})
  } else {
    router.push({ name: 'project-run', params: { projectId } }).catch(() => {})
  }
}

function closeTab(projectId: string, e?: MouseEvent) {
  e?.stopPropagation()
  const wasActive = projectId === activeProjectId.value
  const next = tabsStore.close(projectId)
  if (!wasActive) return
  if (next) {
    select(next.projectId)
  } else {
    router.push({ name: 'projects' }).catch(() => {})
  }
}

/** Close other / right / all tabs. Losing the tab on screen lands on the one right-clicked. */
function closeTabs(projectId: string, scope: TabCloseScope) {
  const ids = tabsToClose(tabsStore.tabs.map((tab) => tab.projectId), projectId, scope)
  const active = activeProjectId.value
  for (const id of ids) tabsStore.close(id)
  if (!active || !ids.includes(active)) return
  if (tabsStore.has(projectId)) select(projectId)
  else router.push({ name: 'projects' }).catch(() => {})
}

// --- Drag & drop reorder ---------------------------------------------------
// The tab strip is a single-row case of the shared sortable; the whole tab is
// the handle (a tab is too small for a separate grip), so a press only becomes
// a drag past a few pixels of travel — below that it stays an ordinary click
// that switches project.
const stripRef = ref<HTMLElement | null>(null)
const { dragFrom, start: startDrag, itemStyle: tabStyle, clickWasDrag } = useDragSort({
  container: stripRef,
  onDrop: (from, to) => tabsStore.move(from, to),
})

/** Click after a drag means "drop here", not "switch to this project". */
function onTabClick(projectId: string) {
  if (clickWasDrag()) return
  select(projectId)
}

// --- Right-click menu ------------------------------------------------------
const menu = ref<{ projectId: string; x: number; y: number } | null>(null)
const menuProject = computed(() => (menu.value ? projectOf(menu.value.projectId) : undefined))
const menuIndex = computed(() =>
  menu.value ? tabsStore.tabs.findIndex((tab) => tab.projectId === menu.value?.projectId) : -1,
)

function openMenu(projectId: string, e: MouseEvent) {
  menu.value = { projectId, x: e.clientX, y: e.clientY }
}

// Until the quick view (UX-52) exists, both rows go to the pages it will
// replace: the Projects page with this project selected, and its session
// defaults section.
function viewProject(projectId: string) {
  router.push({ name: 'projects', query: { id: projectId } }).catch(() => {})
}
function openSessionDefaults(projectId: string) {
  router.push({ name: 'project-settings', params: { projectId }, query: { section: 'session' } }).catch(() => {})
}

async function setColor(color: ProjectColor | null) {
  const project = menuProject.value
  if (!project || projectColor(project.color) === color) return
  try {
    // Saves and refetches the projects, so the tab and the Projects list repaint at once.
    await projectsStore.updateAppearance(project, { color })
  } catch (e) {
    toast.error(String(e))
  }
}

async function openInFolder(path: string) {
  try {
    await projectsStore.openInFolder(path)
  } catch (e) {
    toast.error(String(e))
  }
}

// --- Overflow ▾ ------------------------------------------------------------
// Shown only once the tabs no longer fit, listing every open tab to jump to.
// The strip's own size doesn't change as tabs are added past the row's width
// (only its scroll width does), so it is re-measured on tab changes too.
const overflowing = ref(false)

function measureOverflow() {
  const el = stripRef.value
  overflowing.value = !!el && el.scrollWidth > el.clientWidth + 1
}

let resizeObserver: ResizeObserver | null = null
watch(stripRef, (el) => {
  resizeObserver?.disconnect()
  resizeObserver = null
  if (!el || typeof ResizeObserver === 'undefined') return
  resizeObserver = new ResizeObserver(measureOverflow)
  resizeObserver.observe(el)
})
watch(
  () => tabsStore.tabs.map((tab) => projectName(tab.projectId)).join('\n'),
  measureOverflow,
  { flush: 'post' },
)
onBeforeUnmount(() => resizeObserver?.disconnect())

async function jumpTo(projectId: string) {
  select(projectId)
  await nextTick()
  const index = tabsStore.tabs.findIndex((tab) => tab.projectId === projectId)
  stripRef.value
    ?.querySelector<HTMLElement>(`[data-drag-index="${index}"]`)
    ?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
}
</script>

<template>
  <!-- Lives inside the app title bar (see App.vue), which owns the row's height,
       background and bottom border — hence none of those here. Not flex-1: the
       project switcher's button follows the last tab (or the overflow ▾); the
       strip still shrinks and scrolls once the tabs outgrow the row. -->
  <div v-if="tabsStore.tabs.length > 0" class="flex items-center gap-1 min-w-0 h-full">
    <div
      ref="stripRef"
      class="flex items-center gap-1 min-w-0 h-full overflow-x-auto overflow-y-hidden"
    >
      <button
        v-for="(tab, index) in tabsStore.tabs"
        :key="tab.projectId"
        type="button"
        :data-drag-index="index"
        class="group relative flex h-7 min-w-0 shrink items-center gap-2 pl-3 pr-2 rounded-md text-sm max-w-[200px] transition-colors select-none touch-none"
        :class="[
          dragFrom === index
            ? 'bg-accent text-foreground shadow-md cursor-grabbing'
            : tab.projectId === activeProjectId
              ? 'bg-accent text-foreground cursor-pointer'
              : 'text-muted-foreground hover:text-foreground hover:bg-accent/50 cursor-pointer',
        ]"
        :style="tabStyle(index)"
        :title="`${projectName(tab.projectId)} — ${t('misc.tabs.dragToReorder')}`"
        @pointerdown="startDrag(index, $event)"
        @click="onTabClick(tab.projectId)"
        @contextmenu.prevent="openMenu(tab.projectId, $event)"
      >
        <!-- project colour dot; amber = awaiting permission, ping = running -->
        <span class="relative flex h-2 w-2 shrink-0">
          <span
            v-if="stateOf(tab.projectId).running"
            class="absolute inline-flex h-full w-full rounded-full opacity-60 animate-ping"
            :class="tabLooks[tab.projectId].swatch || 'bg-primary'"
          />
          <span class="relative inline-flex h-2 w-2 rounded-full" :class="dotClass(tab.projectId)" />
        </span>

        <component
          :is="tabLooks[tab.projectId].icon"
          class="h-[13px] w-[13px] shrink-0"
          :class="tabLooks[tab.projectId].tint || 'opacity-70'"
          :stroke-width="1.75"
        />
        <span class="truncate">{{ projectName(tab.projectId) }}</span>

        <!-- pending-permission badge (summed across the project's runs) -->
        <span
          v-if="stateOf(tab.projectId).pending > 0"
          class="flex h-4 min-w-[16px] items-center justify-center rounded-full bg-warning px-1 text-2xs font-medium text-warning-foreground leading-none shrink-0"
        >{{ stateOf(tab.projectId).pending }}</span>

        <!-- close -->
        <span
          class="flex h-4 w-4 items-center justify-center rounded shrink-0 opacity-0 group-hover:opacity-100 hover:bg-foreground/10 transition-opacity"
          :class="{ 'opacity-60': tab.projectId === activeProjectId }"
          role="button"
          :aria-label="t('misc.tabs.closeTab')"
          @pointerdown.stop
          @click="closeTab(tab.projectId, $event)"
        >
          <X class="h-3 w-3" :stroke-width="2" />
        </span>
      </button>
    </div>

    <DropdownMenu v-if="overflowing" align="left">
      <template #trigger>
        <Button
          variant="ghost"
          size="icon-sm"
          class="shrink-0"
          :title="t('misc.tabs.allTabs')"
          :aria-label="t('misc.tabs.allTabs')"
        >
          <ChevronDown class="h-4 w-4" :stroke-width="1.75" />
        </Button>
      </template>
      <DropdownItem v-for="tab in tabsStore.tabs" :key="tab.projectId" @click="jumpTo(tab.projectId)">
        <span class="h-2 w-2 shrink-0 rounded-full" :class="dotClass(tab.projectId)" />
        <span class="max-w-[240px] truncate" :title="projectName(tab.projectId)">{{ projectName(tab.projectId) }}</span>
        <Check
          v-if="tab.projectId === activeProjectId"
          class="ml-auto h-3.5 w-3.5 shrink-0 text-primary"
          :stroke-width="2"
        />
      </DropdownItem>
    </DropdownMenu>

    <ContextMenu :open="!!menu" :x="menu?.x ?? 0" :y="menu?.y ?? 0" @close="menu = null">
      <template v-if="menu">
        <DropdownItem @click="newSession(menu.projectId)">
          <Plus class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ t('projectSwitcher.newSession') }}
        </DropdownItem>
        <DropdownSeparator />
        <DropdownItem @click="closeTab(menu.projectId)">
          <X class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ t('misc.tabs.closeTab') }}
        </DropdownItem>
        <DropdownItem :disabled="tabsStore.tabs.length < 2" @click="closeTabs(menu.projectId, 'others')">
          <span class="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          {{ t('misc.tabs.closeOthers') }}
        </DropdownItem>
        <DropdownItem
          :disabled="menuIndex === -1 || menuIndex === tabsStore.tabs.length - 1"
          @click="closeTabs(menu.projectId, 'right')"
        >
          <span class="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          {{ t('misc.tabs.closeRight') }}
        </DropdownItem>
        <DropdownItem @click="closeTabs(menu.projectId, 'all')">
          <span class="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          {{ t('misc.tabs.closeAll') }}
        </DropdownItem>
        <DropdownSeparator />
        <DropdownItem :disabled="!menuProject" @click="viewProject(menu.projectId)">
          <Eye class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ t('misc.tabs.viewProject') }}
        </DropdownItem>
        <DropdownItem :disabled="!menuProject" @click="openSessionDefaults(menu.projectId)">
          <SlidersHorizontal class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ t('misc.tabs.sessionDefaults') }}
        </DropdownItem>
        <template v-if="menuProject">
          <DropdownSeparator />
          <div class="px-2.5 py-1.5">
            <p class="mb-1.5 text-2xs font-medium text-muted-foreground">{{ t('projectDetail.colorLabel') }}</p>
            <div class="flex items-center gap-1.5">
              <button
                v-for="color in PROJECT_COLORS"
                :key="color"
                type="button"
                class="h-4 w-4 rounded-full border-2 transition-transform cursor-pointer"
                :class="[
                  projectColor(menuProject.color) === color ? 'border-foreground scale-110' : 'border-transparent',
                  swatchClass(color),
                ]"
                :title="t(`projectDetail.colors.${color}`)"
                :aria-label="t(`projectDetail.colors.${color}`)"
                :aria-pressed="projectColor(menuProject.color) === color"
                @click="setColor(color)"
              />
              <button
                type="button"
                class="ml-1 flex h-5 w-5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground cursor-pointer disabled:opacity-40 disabled:pointer-events-none"
                :disabled="!projectColor(menuProject.color)"
                :title="t('misc.tabs.clearColor')"
                :aria-label="t('misc.tabs.clearColor')"
                @click="setColor(null)"
              >
                <Ban class="h-3.5 w-3.5" :stroke-width="1.75" />
              </button>
            </div>
          </div>
          <DropdownSeparator />
          <DropdownItem @click="openInFolder(menuProject.path)">
            <FolderOpen class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
            {{ t('projects.openFolder') }}
          </DropdownItem>
        </template>
      </template>
    </ContextMenu>
  </div>
</template>
