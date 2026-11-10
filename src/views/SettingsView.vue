<script setup lang="ts">
// Settings screen: the section nav (with its `?section=` deep link and feature
// gating) and the active section. Each section lives in components/settings/
// (or components/ for the older ones); the `settings` table form they share is
// created here (useSettingsForm) so its auto-save stops with this view.
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import { PageHeader } from '@/components/ui'
import DevicesSettings, { type DevicesTab } from '@/components/DevicesSettings.vue'
import InboxApiSettings from '@/components/InboxApiSettings.vue'
import FocusSettings from '@/components/FocusSettings.vue'
import FeaturesSettings from '@/components/FeaturesSettings.vue'
import CommandPaletteSettings from '@/components/CommandPaletteSettings.vue'
import KeymapSettings from '@/components/KeymapSettings.vue'
import GeneralSettings from '@/components/settings/GeneralSettings.vue'
import ConnectionsSettings from '@/components/settings/ConnectionsSettings.vue'
import EnginesSettings from '@/components/settings/EnginesSettings.vue'
import SessionDefaultsSettings from '@/components/settings/SessionDefaultsSettings.vue'
import TranslateSettings from '@/components/settings/TranslateSettings.vue'
import SessionsSettings from '@/components/settings/SessionsSettings.vue'
import ResponseStylesSettings from '@/components/settings/ResponseStylesSettings.vue'
import { createSettingsForm } from '@/components/settings/useSettingsForm'
import { useConnectionSources } from '@/components/settings/useConnectionSources'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useFeaturesStore } from '@/stores/features'
import { SETTINGS_SECTIONS, settingsNav, type SettingsSectionId } from '@/lib/settingsSections'
import { isConnectionTypeId, type ConnectionFilter } from '@/lib/connectionTypes'
import { useBudgetStore } from '@/stores/budget'

const { t } = useI18n()

const budget = useBudgetStore()
const { loading, load } = createSettingsForm()

// The detailed subscription plan-usage breakdown now lives in the Stats view.
// Settings keeps the `plan_usage_updated` listener only to keep the budget badge
// (which derives from the same /usage data) in sync.
let unlistenPlanUsage: UnlistenFn | null = null
let unlistenBudgetStatus: UnlistenFn | null = null

const SECTIONS = SETTINGS_SECTIONS
const activeSection = ref<SettingsSectionId>('general')

// Allow deep-linking to a specific section via `/settings?section=google` (used
// by the "Google settings" button on the MCP Servers screen's built-in row).
const route = useRoute()
const devicesTab = ref<DevicesTab>('remote')
// Remote Control and Backup (once Sync) were sections of their own: now tabs of `devices`.
const DEVICES_TAB_OF: Record<string, DevicesTab> = { remote: 'remote', sync: 'backup', backup: 'backup' }
// Claude Accounts and Engine Paths were sections of their own: now the engine cards of `engines`.
// Usage & Budget is now AI › Sessions.
const MERGED_INTO: Record<string, SettingsSectionId> = { claude: 'engines', engine: 'engines', usage: 'sessions' }
// GitHub, GitLab, AWS, Azure and Google were sections too: now Connections, filtered to that type.
const connectionsFilter = ref<ConnectionFilter>('all')
function applySectionFromRoute() {
  let target = route.query.section
  if (typeof target === 'string' && MERGED_INTO[target]) target = MERGED_INTO[target]
  if (typeof target === 'string' && DEVICES_TAB_OF[target]) {
    devicesTab.value = DEVICES_TAB_OF[target]
    target = 'devices'
  }
  if (isConnectionTypeId(target)) {
    connectionsFilter.value = target
    target = 'connections'
  }
  if (typeof target === 'string' && SECTIONS.some(s => s.id === target)) {
    activeSection.value = target as SettingsSectionId
  }
}
applySectionFromRoute()
watch(() => route.query.section, applySectionFromRoute)

// Stores behind Connections (GitHub / GitLab / AWS) and Claude: fetched here, before
// the skeleton goes, so their lists and the nav badge are filled on first paint.
// Connections fetches Azure and Google itself; the badge counts every enabled type.
const ghStore = useGithubAccountsStore()
const glStore = useGitlabAccountsStore()
const awsStore = useAwsAccountsStore()
const claudeStore = useClaudeAccountsStore()
const { total: connectionsCount } = useConnectionSources()

onMounted(async () => {
  try {
    await load()
    await ghStore.fetch()
    await glStore.fetch()
    await awsStore.fetch()
    await claudeStore.fetch()
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
const nav = computed(() => settingsNav(SECTIONS, features.tree))
const visibleSections = computed(() => nav.value.flatMap((g) => g.sections))
watch([visibleSections, activeSection, () => features.loaded], ([list, active, loaded]) => {
  if (loaded && !list.some((s) => s.id === active)) activeSection.value = 'general'
}, { immediate: true })
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Header -->
    <PageHeader :title="t('settings.title')" />

    <!-- Content -->
    <div class="flex-1 flex min-h-0">
      <!-- Section nav -->
      <nav class="w-48 shrink-0 border-r border-border/60 p-3 overflow-auto">
        <div v-for="g in nav" :key="g.id" class="mb-3 last:mb-0">
          <div class="px-2.5 pb-1 text-2xs font-medium uppercase tracking-wider text-muted-foreground/70">
            {{ t(g.labelKey) }}
          </div>
          <button
            v-for="s in g.sections"
            :key="s.id"
            class="w-full flex items-center gap-2.5 px-2.5 py-2 mb-0.5 text-xs rounded-md transition-colors cursor-pointer text-left"
            :class="activeSection === s.id
              ? 'bg-accent text-foreground font-medium'
              : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'"
            @click="activeSection = s.id"
          >
            <component :is="s.icon" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
            <span class="truncate">{{ t(s.labelKey) }}</span>
            <span
              v-if="s.id === 'connections' && connectionsCount"
              class="ml-auto text-2xs tabular-nums text-muted-foreground"
            >{{ connectionsCount }}</span>
          </button>
        </div>
      </nav>

      <!-- Active section panel -->
      <div class="flex-1 overflow-auto p-6 min-w-0">
        <!-- Loading skeleton -->
        <div v-if="loading" class="max-w-lg space-y-4">
          <div v-for="i in 3" :key="i" class="h-28 bg-card rounded-lg border border-border animate-pulse" />
        </div>

        <div v-else class="max-w-lg">

        <GeneralSettings v-show="activeSection === 'general'" />
        <ConnectionsSettings v-show="activeSection === 'connections'" v-model:filter="connectionsFilter" />
        <EnginesSettings v-show="activeSection === 'engines'" @open-section="activeSection = $event" />
        <SessionDefaultsSettings v-show="activeSection === 'ai'" @open-section="activeSection = $event" />
        <ResponseStylesSettings v-if="activeSection === 'styles'" />
        <TranslateSettings v-show="activeSection === 'translate'" />
        <SessionsSettings v-show="activeSection === 'sessions'" />

        </div>

        <!-- Remote Control + Sync & backup under their shared relay / device name / master password
             (wider than the max-w-lg forms to fit the audit table) -->
        <div v-if="!loading && activeSection === 'devices'" class="max-w-3xl">
          <DevicesSettings v-model:tab="devicesTab" />
        </div>

        <!-- Focus board: working hours. Mounted only while shown so it reads the live settings. -->
        <div v-if="!loading && activeSection === 'focus'" class="max-w-lg">
          <FocusSettings />
        </div>

        <!-- Inbox API for the Chrome extension (Slack thread links + translation). Mounted only while
             shown, so opening the section always reads the live port / status. -->
        <div v-if="!loading && activeSection === 'inbox'" class="max-w-lg">
          <InboxApiSettings />
        </div>

        <!-- Module → prop switches. Mounted only while shown: it re-reads the tree on open. -->
        <div v-if="!loading && activeSection === 'features'" class="max-w-lg">
          <FeaturesSettings />
        </div>

        <!-- Command palette (⌘K): groups, pins, hidden items. -->
        <div v-if="!loading && activeSection === 'palette'" class="max-w-lg">
          <CommandPaletteSettings />
        </div>

        <!-- Shortcuts of the native menu's actions (lib/keymap). -->
        <div v-if="!loading && activeSection === 'keymap'" class="max-w-lg">
          <KeymapSettings />
        </div>
      </div>
    </div>
  </div>
</template>
