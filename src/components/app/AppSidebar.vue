<script setup lang="ts">
// The main window's sidebar: three headed groups (Work / My work / Agent), the
// docks, and a footer with Stats, Trash, Settings and the version. App.vue
// decides whether it shows (`visible`) and in which form (`mode`): the full
// 216px column, or a 52px icon rail (same entries, labels as tooltips).
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { getVersion } from '@tauri-apps/api/app'
import { ListTodo, LockKeyhole, Slack, StickyNote, Trash2 } from 'lucide-vue-next'
import { useProjectsStore } from '@/stores/projects'
import { useFocusStore } from '@/stores/focus'
import { useFeaturesStore } from '@/stores/features'
import { useUILayoutStore } from '@/stores/uiLayout'
import ActiveRunsDock from '@/components/ActiveRunsDock.vue'
import BackgroundProcessesDock from '@/components/processes/BackgroundProcessesDock.vue'
import RemoteIndicator from '@/components/RemoteIndicator.vue'
import RemoteDesktopIndicator from '@/components/remote/RemoteDesktopIndicator.vue'
import SidebarNavItem from '@/components/app/SidebarNavItem.vue'
import SidebarRailRuns from '@/components/app/SidebarRailRuns.vue'
import type { NavBadge } from '@/components/app/SidebarBadge.vue'
import { openSlackWindow } from '@/lib/slackWindow'
import { openItemListWindow } from '@/lib/itemListWindow'
import { openSecretNotes } from '@/lib/secretNotesWindow'
import { NAV_ROUTES, type NavRoute } from '@/lib/navigation'
import type { SidebarMode } from '@/lib/sidebarMode'
import { navActive, sidebarGroups, type SidebarSource } from '@/lib/sidebarNav'
import { useOpenModes } from '@/composables/useOpenModes'
import { useRouteCaptureContext } from '@/composables/useRouteCaptureContext'

const props = defineProps<{
  /** Off while focus mode collapses a run down to its conversation. */
  visible: boolean
  mode: SidebarMode
}>()
const rail = computed(() => props.mode === 'rail')

const route = useRoute()
const router = useRouter()
const projectsStore = useProjectsStore()
// Focus board for the sidebar badge (number of overdue todos).
const focusStore = useFocusStore()
const features = useFeaturesStore()
const uiLayout = useUILayoutStore()
const { t } = useI18n()
const routeCaptureContext = useRouteCaptureContext()
const { isInOwnWindow } = useOpenModes()

// App version shown in the sidebar footer; read from Tauri so it always matches
// the packaged build (tauri.conf.json) instead of a hardcoded string.
const appVersion = ref('')
onMounted(async () => {
  try {
    appVersion.value = await getVersion()
  } catch {
    // Ignore (e.g. running outside the Tauri shell during dev in a browser).
  }
})

interface Entry extends SidebarSource {
  label: string
  icon: NavRoute['icon']
  /** Route to go to; unset for a window button. */
  to?: string
  /** Opens a window instead of navigating. */
  action?: () => unknown
  screen?: NavRoute['screen']
}

// Todos, Notes, Secret notes and Slack threads are overlays, not destinations:
// each opens its own window so whatever screen (or running session) is
// underneath survives untouched. Trash is reached from here and the palette.
const extras = computed<Entry[]>(() => [
  {
    id: 'todos', group: 'mine', feature: 'productivity.todos', label: t('item.navTodos'), icon: ListTodo, screen: 'tasks',
    action: () => openItemListWindow('todo', { projectId: routeCaptureContext().projectId }),
  },
  {
    id: 'notes', group: 'mine', feature: 'productivity.notes', label: t('item.navNotes'), icon: StickyNote, screen: 'notes',
    action: () => openItemListWindow('note', { projectId: routeCaptureContext().projectId }),
  },
  {
    id: 'secret-notes', group: 'mine', feature: 'security.secret_vault', label: t('secretNotes.nav'), icon: LockKeyhole, screen: 'secret-notes',
    action: () => openSecretNotes(router, { projectId: routeCaptureContext().projectId }),
  },
  {
    id: 'slack', group: 'mine', feature: 'integrations.slack', label: t('slackThread.nav'), icon: Slack, screen: 'slack',
    action: () => openSlackWindow({ projectId: routeCaptureContext().projectId }),
  },
  { id: '/trash', group: 'foot', label: t('nav.trash'), icon: Trash2, to: '/trash' },
])

// Same routes as the native "Go" menu and ⌘K (see lib/navigation). Until the
// feature switches load only ungated entries show, so a gated one never flashes.
const groups = computed(() => {
  const routes: Entry[] = NAV_ROUTES.map((r) => ({
    id: r.path,
    group: r.group,
    feature: r.feature,
    sidebar: r.sidebar,
    label: t(r.labelKey),
    icon: r.icon,
    to: r.path,
    action: r.openWindow,
    screen: r.screen,
  }))
  return sidebarGroups([...routes, ...extras.value], features.loaded ? features.tree : [])
})
const mainGroups = computed(() => groups.value.filter((g) => g.id !== 'foot'))
const footItems = computed(() => groups.value.find((g) => g.id === 'foot')?.items ?? [])

function badgeFor(id: string): NavBadge | null {
  if (id === '/focus') return { count: focusStore.overdueCount, tone: 'danger', title: t('focus.buckets.overdue') }
  if (id === '/projects') {
    return { count: projectsStore.conflicts.length + projectsStore.ruleConflicts.length, tone: 'warning' }
  }
  return null
}

// The rail drops the number from the icon; it moves into the tooltip.
function badgeTip(id: string): string | undefined {
  const b = badgeFor(id)
  if (!b || b.count <= 0) return undefined
  return id === '/focus' ? t('nav.rail.overdue', { n: b.count }) : t('nav.rail.conflicts', { n: b.count }, b.count)
}
</script>

<template>
  <aside
    v-if="visible"
    class="app-sidebar shrink-0 overflow-hidden bg-sidebar border-r border-border/50"
    :class="rail ? 'w-[52px]' : 'w-[216px]'"
  >
    <div class="h-full flex flex-col" :class="rail ? 'w-[52px]' : 'w-[216px]'">
      <!-- No brand header: the logo moved to the title bar, so the nav starts
           at the very top and a view's header owns the first line alone. -->
      <nav
        class="flex-1 min-h-0 overflow-y-auto py-2.5"
        :class="rail ? 'px-2 space-y-1' : 'px-2 space-y-3'"
      >
        <section
          v-for="(group, i) in mainGroups"
          :key="group.id"
          :class="rail ? ['space-y-0.5', i > 0 && 'border-t border-border pt-1'] : 'space-y-0.5'"
        >
          <h2 v-if="group.labelKey && !rail" class="px-3 pb-1 text-[13px] font-medium text-muted-foreground">
            {{ t(group.labelKey) }}
          </h2>
          <SidebarNavItem
            v-for="item in group.items"
            :key="item.id"
            :label="item.label"
            :icon="item.icon"
            :to="item.to"
            :action="item.action"
            :active="navActive(item.id, route.path)"
            :in-own-window="!!item.screen && isInOwnWindow(item.screen)"
            :badge="badgeFor(item.id)"
            :compact="rail"
            :badge-tip="badgeTip(item.id)"
          />
        </section>
      </nav>

      <!-- The rail has no room for the docks: one icon stands in for the runs. -->
      <div v-if="rail" class="px-2 pb-1">
        <SidebarRailRuns />
      </div>
      <template v-else>
        <!-- App-wide monitor of concurrent runs + permission center -->
        <ActiveRunsDock :mode="uiLayout.dockMode" @toggle="uiLayout.toggleDock()" />

        <!-- Warning row while processes outlive their session -->
        <BackgroundProcessesDock />
      </template>

      <!-- Stats, Trash, Settings: kept at the bottom whatever the nav scrolls. -->
      <div class="space-y-0.5 px-2 pb-1.5" :class="rail && 'border-t border-border pt-1'">
        <SidebarNavItem
          v-for="item in footItems"
          :key="item.id"
          :label="item.label"
          :icon="item.icon"
          :to="item.to"
          :action="item.action"
          :active="navActive(item.id, route.path)"
          :in-own-window="!!item.screen && isInOwnWindow(item.screen)"
          :compact="rail"
        />
      </div>

      <!-- Version footer; the rail keeps only the Remote dot. -->
      <div
        class="flex items-center border-t border-border/50"
        :class="rail ? 'justify-center py-3' : 'justify-between gap-2 px-4 py-3'"
      >
        <button
          v-if="appVersion && !rail"
          type="button"
          class="-mx-1.5 h-7 rounded-md px-1.5 text-2xs text-muted-foreground/70 font-mono hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          :title="t('about.openHint')"
          @click="router.push('/settings/about').catch(() => {})"
        >
          v{{ appVersion }}
        </button>
        <!-- This machine's Remote Control dot, or (remote-desktop window) the
             link to the controlled Puppeteer; each hides itself elsewhere. -->
        <RemoteIndicator :compact="rail" />
        <RemoteDesktopIndicator :compact="rail" />
      </div>
    </div>
  </aside>
</template>
