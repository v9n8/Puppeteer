<script setup lang="ts">
// Settings: a screen of the main window (`/settings/<section>`) or, `windowed`,
// the body of its own window (views/SettingsWindow.vue, lib/settingsWindow) —
// where the user keeps it (lib/detachedScreens). The section nav (feature
// gating) and the active section. On the route the section is the URL; in the
// window, the `place` model. Each section lives in components/settings/ (or
// components/ for the older ones); the `settings` table form they share is
// created here (useSettingsForm) so its auto-save stops when the screen goes.
import { ref, nextTick, onMounted, onUnmounted, computed, watch } from 'vue'
import { onBeforeRouteLeave, onBeforeRouteUpdate, useRoute, useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import DetachButton from '@/components/app/DetachButton.vue'
import MergeBackButton from '@/components/app/MergeBackButton.vue'
import { InfoTip } from '@/components/ui'
import DevicesSettings, { type DevicesTab } from '@/components/DevicesSettings.vue'
import InboxApiSettings from '@/components/InboxApiSettings.vue'
import FocusSettings from '@/components/FocusSettings.vue'
import FeaturesSettings from '@/components/FeaturesSettings.vue'
import GeneralSettings from '@/components/settings/GeneralSettings.vue'
import ConnectionsSettings from '@/components/settings/ConnectionsSettings.vue'
import EnginesSettings from '@/components/settings/EnginesSettings.vue'
import SessionDefaultsSettings from '@/components/settings/SessionDefaultsSettings.vue'
import TranslateSettings from '@/components/settings/TranslateSettings.vue'
import LimitsSettings, { type LimitsTab } from '@/components/settings/LimitsSettings.vue'
import SettingsSearch from '@/components/settings/SettingsSearch.vue'
import { revealSettingsRow } from '@/components/settings/revealSettingsRow'
import ResponseStylesSettings from '@/components/settings/ResponseStylesSettings.vue'
import FlagsSettings from '@/components/settings/FlagsSettings.vue'
import MemorySettings from '@/components/settings/MemorySettings.vue'
import AboutSettings from '@/components/settings/AboutSettings.vue'
import ModelSettings from '@/components/settings/ModelSettings.vue'
import McpSettings from '@/components/settings/McpSettings.vue'
import ShortcutsSettings, { type ShortcutsTab } from '@/components/settings/ShortcutsSettings.vue'
import { createSettingsForm } from '@/components/settings/useSettingsForm'
import { useConnectionSources } from '@/components/settings/useConnectionSources'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { useFeaturesStore } from '@/stores/features'
import {
  SETTINGS_SECTIONS, resolveSettingsPlace, settingsCrumbKeys, settingsHref, settingsLocation, settingsNav, settingsPlaceFromRoute,
  type SettingsPlace, type SettingsSectionId,
} from '@/lib/settingsSections'
import { openSettingsWindow } from '@/lib/settingsWindow'
import type { ScreenTarget } from '@/lib/detachedScreens'
import type { ConnectionFilter } from '@/lib/connectionTypes'
import type { SearchEntry } from '@/lib/settingsSearch'
import { useBudgetStore } from '@/stores/budget'

const props = defineProps<{
  /** Rendered in its own window: no router, the section is the `place` model. */
  windowed?: boolean
}>()
/** The window's section (windowed only; on the route the URL holds it). */
const windowPlace = defineModel<SettingsPlace>('place', { default: () => ({ section: 'general' }) })

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const budget = useBudgetStore()
const { loading, load } = createSettingsForm()

// The detailed subscription plan-usage breakdown now lives in the Stats view.
// Settings keeps the `plan_usage_updated` listener only to keep the budget badge
// (which derives from the same /usage data) in sync.
let unlistenPlanUsage: UnlistenFn | null = null
let unlistenBudgetStatus: UnlistenFn | null = null

const place = computed<SettingsPlace>(() =>
  props.windowed ? windowPlace.value : settingsPlaceFromRoute(route.params, route.query),
)
const activeSection = computed<SettingsSectionId>(() => place.value.section)

// A link with a tab or filter (`/settings/connections?type=google`, e.g. the
// "Google settings" button on the MCP Servers screen's built-in row) opens on it.
const devicesTab = ref<DevicesTab>('remote')
const shortcutsTab = ref<ShortcutsTab>('palette')
const connectionsFilter = ref<ConnectionFilter>('all')
// The Limits tab is not in the URL: the search sets it before it opens the section.
const limitsTab = ref<LimitsTab>('context')
watch(place, (p) => {
  if (p.devicesTab) devicesTab.value = p.devicesTab
  if (p.shortcutsTab) shortcutsTab.value = p.shortcutsTab
  if (p.connectionsFilter) connectionsFilter.value = p.connectionsFilter
}, { immediate: true })

/** Where the user is now, tab and filter included: what the other window opens on. */
const currentPlace = computed<SettingsPlace>(() => {
  const p: SettingsPlace = { section: activeSection.value }
  if (p.section === 'devices') p.devicesTab = devicesTab.value
  if (p.section === 'shortcuts') p.shortcutsTab = shortcutsTab.value
  if (p.section === 'connections' && connectionsFilter.value !== 'all') p.connectionsFilter = connectionsFilter.value
  return p
})

// The Shortcuts tab is in the URL (`?tab=`), so a reload or a shared link keeps it.
watch(shortcutsTab, (tab) => {
  if (props.windowed || activeSection.value !== 'shortcuts' || route.query.tab === tab) return
  void router.replace(settingsLocation({ section: 'shortcuts', shortcutsTab: tab })).catch(() => {})
})

// Stores behind Connections (GitHub / GitLab / AWS), Claude and Codex: fetched here, before
// the skeleton goes, so their lists and the nav badge are filled on first paint.
// Connections fetches Azure and Google itself; the badge counts every enabled type.
const ghStore = useGithubAccountsStore()
const glStore = useGitlabAccountsStore()
const awsStore = useAwsAccountsStore()
const claudeStore = useClaudeAccountsStore()
const codexStore = useCodexAccountsStore()
const { total: connectionsCount } = useConnectionSources()

onMounted(async () => {
  try {
    await load()
    await ghStore.fetch()
    await glStore.fetch()
    await awsStore.fetch()
    await claudeStore.fetch()
    await codexStore.fetch()
    budget.refresh()
    unlistenPlanUsage = await listen<{ provider?: string }>('plan_usage_updated', (e) => {
      // Claude plan usage feeds the budget guardrail verdict — keep the badge synced.
      if (e.payload?.provider !== 'codex' && !budget.refreshingPlan) budget.refresh()
    })
    unlistenBudgetStatus = await listen('budget_status_updated', () => budget.refresh())
  } finally {
    loading.value = false
  }
})

onUnmounted(() => {
  if (unlistenPlanUsage) unlistenPlanUsage()
  if (unlistenBudgetStatus) unlistenBudgetStatus()
})

// A section whose feature is off (AWS, Azure, Google) is hidden, and a group left
// empty loses its heading; when the open section is hidden (switched off meanwhile,
// or a deep link), fall back to General.
const features = useFeaturesStore()
const nav = computed(() => settingsNav(SETTINGS_SECTIONS, features.tree))
const visibleSections = computed(() => nav.value.flatMap((g) => g.sections))
watch([visibleSections, activeSection, () => features.loaded], ([list, active, loaded]) => {
  if (loaded && !list.some((s) => s.id === active)) void openPlace({ section: 'general' }, true)
}, { immediate: true })

// Leaving the Memory section (another section, another screen, the other
// window, closing the window) drops the editor's unsaved edits, so it asks
// first, like switching its file does.
const memory = ref<InstanceType<typeof MemorySettings> | null>(null)
// MCP is mounted only while shown: an unsaved server draft asks the same way (V3-51).
const mcp = ref<InstanceType<typeof McpSettings> | null>(null)
async function mayLeave(): Promise<boolean> {
  if (memory.value && !(await memory.value.mayDiscard())) return false
  return mcp.value ? mcp.value.mayDiscard() : true
}
if (!props.windowed) {
  onBeforeRouteUpdate((to, from) => (to.params.section === from.params.section ? true : mayLeave()))
  onBeforeRouteLeave(() => mayLeave())
}

/** Shows `next`: a route change here, the model in the window. `replace`: a fallback, nothing to ask. */
async function openPlace(next: SettingsPlace, replace = false): Promise<void> {
  if (props.windowed) {
    if (!replace && next.section !== activeSection.value && !(await mayLeave())) return
    windowPlace.value = next
    return
  }
  const loc = settingsLocation(next)
  await (replace ? router.replace(loc) : router.push(loc)).catch(() => {})
}
defineExpose({ openPlace, mayLeave })

/** A search result: open its section (and tab), then scroll to and light the row. */
async function openSearchResult(entry: SearchEntry): Promise<void> {
  const next: SettingsPlace = { section: entry.section }
  if (entry.tab) {
    if (entry.section === 'devices') next.devicesTab = entry.tab as 'remote' | 'backup'
    if (entry.section === 'shortcuts') next.shortcutsTab = entry.tab as 'palette' | 'keymap'
    if (entry.section === 'limits') limitsTab.value = entry.tab as LimitsTab
  }
  await openPlace(next)
  if (!entry.isSection && activeSection.value === entry.section) {
    await nextTick()
    revealSettingsRow(entry.id)
  }
}

// Header / title bar: "Settings › group › section".
const crumb = computed(() => settingsCrumbKeys(activeSection.value).map((k) => t(k)))
// Main window: "Open in new window" moves Settings there, on the section shown.
const detachTarget: ScreenTarget = {
  screen: 'settings',
  path: '/settings',
  openWindow: () => openSettingsWindow(currentPlace.value),
}
// Own window: "Move back to main window" opens the main window on the section shown.
const mergePath = computed(() => settingsHref(settingsLocation(currentPlace.value)))
</script>

<template>
  <div class="flex flex-col h-full bg-background">
    <!-- Header (main window, 48px) or title bar (own window): "Settings › group › section".
         No close button: Settings is a screen, left through the sidebar like any other.
         The settings search (V3-58) sits before the button, in both. -->
    <div
      class="flex shrink-0 items-center gap-2 border-b border-border bg-card px-3"
      :class="windowed ? 'h-10' : 'h-12'"
    >
      <div class="flex min-w-0 flex-1 items-center gap-1 whitespace-nowrap">
        <h1 class="shrink-0 text-xs font-bold">{{ crumb[0] }}</h1>
        <span class="text-xs text-muted-foreground" aria-hidden="true">›</span>
        <span class="shrink-0 text-xs text-muted-foreground">{{ crumb[1] }}</span>
        <span class="text-xs text-muted-foreground" aria-hidden="true">›</span>
        <span class="min-w-0 truncate text-xs">{{ crumb[2] }}</span>
      </div>
      <SettingsSearch @pick="openSearchResult" />
      <MergeBackButton
        v-if="windowed"
        screen="settings"
        :path="mergePath"
        :confirm-leave="mayLeave"
      />
      <DetachButton v-else :target="detachTarget" :confirm-leave="mayLeave" />
    </div>

    <!-- Content -->
    <div class="flex-1 flex min-h-0">
      <!-- Section nav: on the card tone, since it sits right beside the app sidebar
           (sidebar tone); the open section like a sidebar item (accent + primary bar). -->
      <nav class="w-56 shrink-0 border-r border-border bg-card p-2 overflow-auto">
        <div v-for="g in nav" :key="g.id">
          <div class="mx-2 mb-1 mt-3 p-0 text-xs font-medium leading-[1.4] text-muted-foreground first:mt-1">
            {{ t(g.labelKey) }}
          </div>
          <button
            v-for="s in g.sections"
            :key="s.id"
            type="button"
            class="relative w-full flex items-center gap-2.5 h-8 px-2 mb-0.5 text-md rounded-sm transition-colors cursor-pointer text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :class="activeSection === s.id
              ? 'bg-accent text-foreground font-medium before:absolute before:left-0 before:inset-y-1.5 before:w-0.5 before:rounded-full before:bg-primary'
              : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
            :aria-current="activeSection === s.id ? 'page' : undefined"
            @click="openPlace({ section: s.id })"
          >
            <component
              :is="s.icon"
              class="h-4 w-4 shrink-0"
              :class="activeSection === s.id && 'text-primary'"
              :stroke-width="1.75"
            />
            <span class="truncate" :title="t(s.labelKey)">{{ t(s.labelKey) }}</span>
            <span
              v-if="s.id === 'connections' && connectionsCount"
              class="ml-auto text-2xs tabular-nums text-muted-foreground"
            >{{ connectionsCount }}</span>
          </button>
        </div>
      </nav>

      <!-- Active section panel -->
      <div class="flex-1 overflow-auto px-4 pt-4 pb-8 min-[901px]:px-6 min-[901px]:pt-5 min-w-0">
        <!-- Loading skeleton -->
        <div v-if="loading" class="max-w-[832px] space-y-4">
          <div v-for="i in 3" :key="i" class="h-28 bg-card rounded-lg border border-border animate-pulse" />
        </div>

        <div v-else class="max-w-[832px]">

        <GeneralSettings v-show="activeSection === 'general'" />
        <ConnectionsSettings v-show="activeSection === 'connections'" :active="activeSection === 'connections'" v-model:filter="connectionsFilter" />
        <EnginesSettings v-show="activeSection === 'engines'" @open-section="openPlace({ section: $event })" />
        <SessionDefaultsSettings v-show="activeSection === 'ai'" @open-section="openPlace({ section: $event })" />
        <ResponseStylesSettings v-if="activeSection === 'styles'" />
        <FlagsSettings v-if="activeSection === 'flags'" />
        <TranslateSettings v-show="activeSection === 'translate'" />
        <LimitsSettings v-show="activeSection === 'limits'" v-model:tab="limitsTab" />

        </div>

        <!-- Memory: CLAUDE.md / AGENTS.md editor, wider than the forms. Kept mounted (v-show) so
             coming back doesn't re-read the file; leaving with unsaved edits asks first (mayLeave). -->
        <div v-if="!loading" v-show="activeSection === 'memory'" class="max-w-4xl">
          <MemorySettings ref="memory" />
        </div>

        <!-- MCP servers: a list-detail frame, as tall as the panel (no max-w). Mounted only while shown,
             so it reads the live list; leaving with an unsaved server asks first (mayLeave). -->
        <div v-if="!loading && activeSection === 'mcp'" class="flex h-full min-h-[28rem] flex-col gap-4">
          <div class="shrink-0">
            <div class="flex items-center gap-1.5">
              <h2 class="text-base font-semibold text-foreground">{{ t('settings.sections.mcp') }}</h2>
              <InfoTip :text="t('settings.mcp.description')" />
            </div>
          </div>
          <div class="min-h-0 flex-1 overflow-hidden rounded-lg border border-border">
            <McpSettings
              ref="mcp"
              :windowed="windowed"
              @open-section="openPlace(resolveSettingsPlace($event) ?? { section: 'general' })"
            />
          </div>
        </div>

        <!-- Remote Control + Sync & backup under their shared relay / device name / master password -->
        <div v-if="!loading && activeSection === 'devices'" class="max-w-4xl">
          <DevicesSettings v-model:tab="devicesTab" />
        </div>

        <!-- Focus board: working hours. Mounted only while shown so it reads the live settings. -->
        <div v-if="!loading && activeSection === 'focus'" class="max-w-4xl">
          <FocusSettings />
        </div>

        <!-- Inbox API for the Chrome extension (Slack thread links + translation). Mounted only while
             shown, so opening the section always reads the live port / status. -->
        <div v-if="!loading && activeSection === 'inbox'" class="max-w-4xl">
          <InboxApiSettings />
        </div>

        <!-- Module → prop switches. Mounted only while shown: it re-reads the tree on open. -->
        <div v-if="!loading && activeSection === 'features'" class="max-w-4xl">
          <FeaturesSettings />
        </div>

        <!-- Which models each engine may use. Kept mounted (v-show) so unsaved switches survive
             a visit to another section (the Save dialog links to them); it re-reads the lists
             each time it is shown, and ⌘S saves only while it is. -->
        <div v-if="!loading" v-show="activeSection === 'model'" class="max-w-4xl">
          <ModelSettings :active="activeSection === 'model'" @open-section="openPlace({ section: $event })" />
        </div>

        <!-- Version, updates and release notes. Mounted only while shown, so `?release=` scrolls on open. -->
        <div v-if="!loading && activeSection === 'about'" class="max-w-4xl">
          <AboutSettings :release="place.release" />
        </div>

        <!-- Command palette (⌘K) and the menu's shortcuts, as two tabs. -->
        <div v-if="!loading && activeSection === 'shortcuts'" class="max-w-4xl">
          <ShortcutsSettings v-model:tab="shortcutsTab" />
        </div>
      </div>
    </div>
  </div>
</template>
