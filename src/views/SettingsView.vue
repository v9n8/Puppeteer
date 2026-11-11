<script setup lang="ts">
// Settings, shown in the modal of components/settings/SettingsModal.vue: the
// section nav (deep link and feature gating) and the active section. Each
// section lives in components/settings/ (or components/ for the older ones);
// the `settings` table form they share is created here (useSettingsForm) so its
// auto-save stops when the modal closes.
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import { X } from 'lucide-vue-next'
import { Button } from '@/components/ui'
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
import MemorySettings from '@/components/settings/MemorySettings.vue'
import { createSettingsForm } from '@/components/settings/useSettingsForm'
import { useConnectionSources } from '@/components/settings/useConnectionSources'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { useFeaturesStore } from '@/stores/features'
import { SETTINGS_SECTIONS, settingsNav, type SettingsSectionId } from '@/lib/settingsSections'
import type { ConnectionFilter } from '@/lib/connectionTypes'
import { useBudgetStore } from '@/stores/budget'
import { useSettingsModal } from '@/composables/useSettingsModal'

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()

const budget = useBudgetStore()
const { loading, load } = createSettingsForm()

// The detailed subscription plan-usage breakdown now lives in the Stats view.
// Settings keeps the `plan_usage_updated` listener only to keep the budget badge
// (which derives from the same /usage data) in sync.
let unlistenPlanUsage: UnlistenFn | null = null
let unlistenBudgetStatus: UnlistenFn | null = null

const activeSection = ref<SettingsSectionId>('general')

// A deep link (`/settings?section=google`, e.g. the "Google settings" button on
// the MCP Servers screen's built-in row) opens on its section, tab or filter.
const { requested } = useSettingsModal()
const devicesTab = ref<DevicesTab>('remote')
const connectionsFilter = ref<ConnectionFilter>('all')
watch(requested, (place) => {
  if (!place) return
  if (place.devicesTab) devicesTab.value = place.devicesTab
  if (place.connectionsFilter) connectionsFilter.value = place.connectionsFilter
  activeSection.value = place.section
  requested.value = null
}, { immediate: true })

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
  if (loaded && !list.some((s) => s.id === active)) activeSection.value = 'general'
}, { immediate: true })

// Closing drops the Memory editor's unsaved edits, so it asks first, like
// switching its file does.
const memory = ref<InstanceType<typeof MemorySettings> | null>(null)
async function mayClose(): Promise<boolean> {
  return memory.value ? memory.value.mayDiscard() : true
}
defineExpose({ mayClose })
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Header -->
    <div class="flex shrink-0 items-center gap-2 border-b border-border px-4 py-3">
      <h2 class="flex-1 text-sm font-semibold">{{ t('settings.title') }}</h2>
      <Button
        variant="ghost"
        size="icon"
        :title="t('common.close')"
        :aria-label="t('common.close')"
        @click="emit('close')"
      >
        <X class="h-4 w-4" :stroke-width="1.75" />
      </Button>
    </div>

    <!-- Content -->
    <div class="flex-1 flex min-h-0">
      <!-- Section nav -->
      <nav class="w-56 shrink-0 border-r border-border/60 p-3 overflow-auto">
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
            <span class="truncate" :title="t(s.labelKey)">{{ t(s.labelKey) }}</span>
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
        <div v-if="loading" class="max-w-4xl space-y-4">
          <div v-for="i in 3" :key="i" class="h-28 bg-card rounded-lg border border-border animate-pulse" />
        </div>

        <div v-else class="max-w-4xl">

        <GeneralSettings v-show="activeSection === 'general'" />
        <ConnectionsSettings v-show="activeSection === 'connections'" v-model:filter="connectionsFilter" />
        <EnginesSettings v-show="activeSection === 'engines'" @open-section="activeSection = $event" />
        <SessionDefaultsSettings v-show="activeSection === 'ai'" @open-section="activeSection = $event" />
        <ResponseStylesSettings v-if="activeSection === 'styles'" />
        <TranslateSettings v-show="activeSection === 'translate'" />
        <SessionsSettings v-show="activeSection === 'sessions'" />

        </div>

        <!-- Memory: CLAUDE.md / AGENTS.md editor, wider than the forms. Kept mounted (v-show) so
             switching section doesn't drop unsaved edits. -->
        <div v-if="!loading" v-show="activeSection === 'memory'" class="max-w-4xl">
          <MemorySettings ref="memory" />
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

        <!-- Command palette (⌘K): groups, pins, hidden items. -->
        <div v-if="!loading && activeSection === 'palette'" class="max-w-4xl">
          <CommandPaletteSettings />
        </div>

        <!-- Shortcuts of the native menu's actions (lib/keymap). -->
        <div v-if="!loading && activeSection === 'keymap'" class="max-w-4xl">
          <KeymapSettings />
        </div>
      </div>
    </div>
  </div>
</template>
