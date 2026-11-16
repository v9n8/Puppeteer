<script setup lang="ts">
// The main window's sidebar: nav, the Todo / Notes / Secret notes / Slack
// window buttons, the docks, Trash and the version footer. App.vue decides
// whether it shows (`visible`); the collapse animation lives here.
import { computed, onMounted, ref } from 'vue'
import { RouterLink, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { getVersion } from '@tauri-apps/api/app'
import { ListTodo, LockKeyhole, Slack, StickyNote, Trash2 } from 'lucide-vue-next'
import { useProjectsStore } from '@/stores/projects'
import { useFocusStore } from '@/stores/focus'
import { useFeaturesStore } from '@/stores/features'
import ActiveRunsDock from '@/components/ActiveRunsDock.vue'
import BackgroundProcessesDock from '@/components/processes/BackgroundProcessesDock.vue'
import RemoteIndicator from '@/components/RemoteIndicator.vue'
import RemoteDesktopIndicator from '@/components/remote/RemoteDesktopIndicator.vue'
import { openSlackWindow } from '@/lib/slackWindow'
import { openItemListWindow } from '@/lib/itemListWindow'
import { openSecretNotesWindow } from '@/lib/secretNotesWindow'
import { NAV_ROUTES } from '@/lib/navigation'
import { filterByFeature, startsGroup } from '@/lib/featureGate'
import { useRouteCaptureContext } from '@/composables/useRouteCaptureContext'

defineProps<{
  /** Off when the user hid the sidebar, or focus mode collapsed a run. */
  visible: boolean
}>()

const route = useRoute()
const projectsStore = useProjectsStore()
// Focus board for the sidebar badge (number of overdue todos).
const focusStore = useFocusStore()
const features = useFeaturesStore()
const { t } = useI18n()
const routeCaptureContext = useRouteCaptureContext()

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

// Same source of truth as the native "Go" menu (see lib/navigation.ts). Empty
// until the feature switches load, so a gated entry never shows, then vanishes.
const navItems = computed(() =>
  features.loaded
    ? filterByFeature(NAV_ROUTES, features.tree).map((r) => ({
        path: r.path,
        label: t(r.labelKey),
        icon: r.icon,
        group: r.group,
        openWindow: r.openWindow,
      }))
    : [],
)
</script>

<template>
  <Transition name="sidebar">
    <aside
      v-if="visible"
      class="app-sidebar shrink-0 overflow-hidden bg-sidebar border-r border-border/50"
    >
      <div class="w-[220px] h-full flex flex-col">
        <!-- No brand header: the logo moved to the title bar, so the nav starts
             at the very top and a view's header owns the first line alone. -->
        <nav class="flex-1 px-2 py-2.5 space-y-0.5">
          <template v-for="(item, index) in navItems" :key="item.path">
            <!-- Todos, Notes and Slack threads are overlays, not destinations:
                 each opens its own window instead of navigating, so whatever
                 screen (or running session) is underneath survives untouched.
                 They close out the daily-work group, right before the config
                 screens. -->
            <template v-if="startsGroup(navItems, index) && item.group === 'config'">
              <button
                v-if="features.isOn('productivity.todos')"
                type="button"
                class="relative flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors cursor-pointer select-none text-muted-foreground hover:text-foreground hover:bg-accent/50"
                :title="t('item.navTodos')"
                @click="openItemListWindow('todo', { projectId: routeCaptureContext().projectId })"
              >
                <ListTodo class="h-[15px] w-[15px] shrink-0" :stroke-width="1.75" />
                <span class="flex-1 truncate text-left">{{ t('item.navTodos') }}</span>
              </button>

              <button
                v-if="features.isOn('productivity.notes')"
                type="button"
                class="relative flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors cursor-pointer select-none text-muted-foreground hover:text-foreground hover:bg-accent/50"
                :title="t('item.navNotes')"
                @click="openItemListWindow('note', { projectId: routeCaptureContext().projectId })"
              >
                <StickyNote class="h-[15px] w-[15px] shrink-0" :stroke-width="1.75" />
                <span class="flex-1 truncate text-left">{{ t('item.navNotes') }}</span>
              </button>

              <!-- Secret notes: their own window. -->
              <button
                v-if="features.isOn('security.secret_vault')"
                type="button"
                class="relative flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors cursor-pointer select-none text-muted-foreground hover:text-foreground hover:bg-accent/50"
                :title="t('secretNotes.nav')"
                @click="openSecretNotesWindow({ projectId: routeCaptureContext().projectId })"
              >
                <LockKeyhole class="h-[15px] w-[15px] shrink-0" :stroke-width="1.75" />
                <span class="flex-1 truncate text-left">{{ t('secretNotes.nav') }}</span>
              </button>

              <!-- Slack threads have their own window. -->
              <button
                v-if="features.isOn('integrations.slack')"
                type="button"
                class="relative flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors cursor-pointer select-none text-muted-foreground hover:text-foreground hover:bg-accent/50"
                :title="t('slackThread.nav')"
                @click="openSlackWindow({ projectId: routeCaptureContext().projectId })"
              >
                <Slack class="h-[15px] w-[15px] shrink-0" :stroke-width="1.75" />
                <span class="flex-1 truncate text-left">{{ t('slackThread.nav') }}</span>
              </button>
            </template>

            <!-- Divider between nav groups (daily work / agent config / app). -->
            <div v-if="startsGroup(navItems, index)" class="mx-3 !my-2 border-t border-border/50" />

            <!-- Destinations with their own window (PR Inbox) open it, like
                 the Slack button above, instead of routing. -->
            <button
              v-if="item.openWindow"
              type="button"
              class="relative flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors cursor-pointer select-none text-muted-foreground hover:text-foreground hover:bg-accent/50"
              :title="`${item.label} — ${t('nav.opensInWindow')}`"
              @click="item.openWindow()"
            >
              <component :is="item.icon" class="h-[15px] w-[15px] shrink-0" :stroke-width="1.75" />
              <span class="flex-1 truncate text-left">{{ item.label }}</span>
              <span class="sr-only">({{ t('nav.opensInWindow') }})</span>
            </button>
            <RouterLink
              v-else
              :to="item.path"
              class="relative flex items-center gap-2.5 px-3 py-2 rounded-md text-sm transition-colors cursor-pointer select-none"
              :class="route.path.startsWith(item.path)
                ? 'is-active bg-accent text-foreground font-medium'
                : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'"
            >
              <span
                v-if="route.path.startsWith(item.path)"
                class="app-nav-indicator absolute left-0 inset-y-[6px] w-[2px] rounded-r-full bg-primary"
              />
              <component :is="item.icon" class="h-[15px] w-[15px] shrink-0" :stroke-width="1.75" />
              <span class="flex-1 truncate">{{ item.label }}</span>
              <span
                v-if="item.path === '/focus' && focusStore.overdueCount > 0"
                class="flex h-4 min-w-[16px] items-center justify-center rounded-full bg-destructive px-1 text-2xs font-medium text-destructive-foreground leading-none"
                :title="t('focus.buckets.overdue')"
              >
                {{ focusStore.overdueCount }}
              </span>
              <span
                v-if="item.path === '/projects' && (projectsStore.conflicts.length + projectsStore.ruleConflicts.length) > 0"
                class="flex h-4 min-w-[16px] items-center justify-center rounded-full bg-warning px-1 text-2xs font-medium text-warning-foreground leading-none"
              >
                {{ projectsStore.conflicts.length + projectsStore.ruleConflicts.length }}
              </span>
            </RouterLink>
          </template>
        </nav>

        <!-- App-wide monitor of concurrent runs + permission center -->
        <ActiveRunsDock />

        <!-- Warning row while processes outlive their session -->
        <BackgroundProcessesDock />

        <!-- Trash: reached from the footer and the command palette, not a
             nav destination of its own. -->
        <div class="px-2 pb-1.5">
          <RouterLink
            to="/trash"
            class="relative flex items-center gap-2.5 px-3 py-2 rounded-md text-sm transition-colors cursor-pointer select-none"
            :class="route.path.startsWith('/trash')
              ? 'is-active bg-accent text-foreground font-medium'
              : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'"
          >
            <span
              v-if="route.path.startsWith('/trash')"
              class="app-nav-indicator absolute left-0 inset-y-[6px] w-[2px] rounded-r-full bg-primary"
            />
            <Trash2 class="h-[15px] w-[15px] shrink-0" :stroke-width="1.75" />
            <span class="flex-1 truncate">{{ t('nav.trash') }}</span>
          </RouterLink>
        </div>

        <!-- Version footer -->
        <div class="flex items-center justify-between gap-2 px-4 py-3 border-t border-border/50">
          <p v-if="appVersion" class="text-2xs text-muted-foreground/50 font-mono">v{{ appVersion }}</p>
          <!-- This machine's Remote Control dot, or (remote-desktop window) the
               link to the controlled Puppeteer; each hides itself elsewhere. -->
          <RemoteIndicator />
          <RemoteDesktopIndicator />
        </div>
      </div>
    </aside>
  </Transition>
</template>

<style scoped>
/* Sidebar collapse/expand. Width is set here rather than as a Tailwind class so
   the transition classes below can override it without a specificity fight. */
.app-sidebar {
  width: 220px;
}

.sidebar-enter-active,
.sidebar-leave-active {
  transition: width 180ms ease, opacity 180ms ease;
}

.sidebar-enter-from,
.sidebar-leave-to {
  width: 0;
  /* The border would otherwise linger as a 1px line at the very end. */
  border-right-width: 0;
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .sidebar-enter-active,
  .sidebar-leave-active {
    transition: none;
  }
}
</style>
