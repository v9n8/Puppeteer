<script setup lang="ts">
// Right column of the Projects page: the selected project's header, its tabs
// (the five settings groups, UX-50) and, under each tab, a row of its sections.
// Management only — nothing here opens a session (D3).
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Code2, Eye, EyeOff, FolderOpen, GanttChartSquare, SquareTerminal } from 'lucide-vue-next'
import { Banner, Button } from '@/components/ui'
import DetailHeader from '@/components/listDetail/DetailHeader.vue'
import ProjectSessionDefaults from '@/components/ProjectSessionDefaults.vue'
import ResponseStylesSettings from '@/components/settings/ResponseStylesSettings.vue'
import ProjectOverview from '@/components/projects/ProjectOverview.vue'
import ProjectBoard from '@/components/projects/ProjectBoard.vue'
import ProjectConnections from '@/components/projects/ProjectConnections.vue'
import ProjectLibrary from '@/components/projects/ProjectLibrary.vue'
import ProjectServers from '@/components/projects/ProjectServers.vue'
import { useToast } from '@/composables/useToast'
import { invoke } from '@/lib/tauri'
import { openGanttWindow } from '@/lib/ganttWindow'
import { projectIcon, projectTileClass } from '@/lib/projectAppearance'
import { PROJECT_TABS, type ProjectPlace } from '@/lib/projectList'
import {
  PROJECT_SECTIONS, projectNav, sectionGroup, tabSection, type ProjectSectionId,
} from '@/lib/projectSections'
import { useProjectsStore, type Project } from '@/stores/projects'
import { useFeaturesStore } from '@/stores/features'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'

const props = defineProps<{ project: Project }>()
const place = defineModel<ProjectPlace>('place', { required: true })

defineSlots<{
  /** Before the project tile (e.g. the button that opens the list on a narrow window). */
  leading?: () => unknown
  /** After the header's own buttons (e.g. the quick view's close button, UX-52). */
  actions?: () => unknown
}>()

const { t } = useI18n()
const { toast } = useToast()
const store = useProjectsStore()
const features = useFeaturesStore()
const tabsStore = useWorkspaceTabsStore()

// Sync conflicts of every project; this one's add a Conflicts section to the Library tab.
store.fetchConflicts().catch(() => {})
store.fetchRuleConflicts().catch(() => {})
const totalConflicts = computed(() =>
  store.conflicts.filter(c => c.project_id === props.project.id).length
  + store.ruleConflicts.filter(c => c.project_id === props.project.id).length,
)

const sections = computed(() =>
  projectNav(PROJECT_SECTIONS, totalConflicts.value > 0).find(g => g.id === place.value.tab)!.sections,
)
const section = computed(() => tabSection(place.value.tab, place.value.section, totalConflicts.value > 0))

function openSection(id: ProjectSectionId) {
  place.value = { tab: sectionGroup(id), section: id }
}

// --- Actions ---
async function openWith(action: () => Promise<void>) {
  try {
    await action()
  } catch (e) {
    toast.error(String(e))
  }
}
const openInVscode = () => openWith(() => store.openInVscode(props.project.path))
const openInFolder = () => openWith(() => store.openInFolder(props.project.path))
const openInTerminal = () => openWith(async () => {
  const settings = await invoke<{ terminal_app: string }>('get_settings')
  await store.openInTerminal(props.project.path, settings.terminal_app)
})

async function toggleHidden() {
  const project = props.project
  const hidden = !project.hidden
  try {
    await store.updateAppearance(project, { hidden })
    // Its tab goes too; opening the project again brings the tab back.
    if (hidden) tabsStore.close(project.id)
    toast.success(t(hidden ? 'projects.toastHidden' : 'projects.toastShown', { name: project.name }))
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <div class="flex min-h-0 min-w-0 flex-1 flex-col">
    <DetailHeader :title="project.name">
      <template #leading>
        <slot name="leading" />
        <span
          class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
          :class="projectTileClass(project.color)"
        >
          <component :is="projectIcon(project.icon)" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </span>
      </template>
      <template #meta>
        <span class="font-mono" :title="project.path">{{ project.path }}</span>
      </template>
      <template #actions>
        <Button variant="outline" size="icon" :title="t('projects.openInVscode')" :aria-label="t('projects.openInVscode')" @click="openInVscode">
          <Code2 class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button variant="outline" size="icon" :title="t('projects.openFolder')" :aria-label="t('projects.openFolder')" @click="openInFolder">
          <FolderOpen class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button variant="outline" size="icon" :title="t('projects.openInTerminal')" :aria-label="t('projects.openInTerminal')" @click="openInTerminal">
          <SquareTerminal class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button
          v-if="features.allows('projects.gantt')"
          variant="outline"
          size="icon"
          :title="t('projects.gantt')"
          :aria-label="t('projects.gantt')"
          @click="openGanttWindow(project.id)"
        >
          <GanttChartSquare class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button
          variant="outline"
          size="icon"
          :title="project.hidden ? t('projects.unhideProject') : t('projects.hideProject')"
          :aria-label="project.hidden ? t('projects.unhideProject') : t('projects.hideProject')"
          @click="toggleHidden"
        >
          <component :is="project.hidden ? Eye : EyeOff" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <slot name="actions" />
      </template>
    </DetailHeader>

    <div class="flex shrink-0 overflow-x-auto border-b border-border/60 px-5 text-xs font-medium" role="tablist">
      <button
        v-for="id in PROJECT_TABS"
        :key="id"
        type="button"
        role="tab"
        :aria-selected="place.tab === id"
        class="shrink-0 cursor-pointer whitespace-nowrap px-3 py-2 transition-colors"
        :class="place.tab === id ? 'border-b-2 border-primary text-foreground' : 'text-muted-foreground hover:text-foreground'"
        @click="place = { tab: id, section: null }"
      >
        {{ t(`projectDetail.groups.${id}`) }}
      </button>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto px-5 pb-6">
      <!-- Sections of the tab -->
      <nav class="flex flex-wrap gap-1 pt-3" :aria-label="t(`projectDetail.groups.${place.tab}`)">
        <button
          v-for="s in sections"
          :key="s.id"
          type="button"
          class="flex cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-md px-2.5 py-1.5 text-xs transition-colors"
          :class="s.id === 'conflicts'
            ? (section === s.id ? 'bg-warning/15 text-warning font-medium' : 'text-warning hover:bg-warning/10')
            : (section === s.id ? 'bg-accent text-foreground font-medium' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground')"
          :aria-current="section === s.id ? 'page' : undefined"
          @click="place = { tab: place.tab, section: s.id }"
        >
          <component :is="s.icon" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ t('projectDetail.sections.' + s.id) }}
          <span v-if="s.id === 'conflicts'" class="text-2xs tabular-nums">{{ totalConflicts }}</span>
        </button>
      </nav>

      <Banner v-if="totalConflicts > 0 && section !== 'conflicts'" tone="warning" class="mt-3 max-w-lg">
        <p class="text-xs font-medium text-warning">{{ t('projectDetail.conflictBannerTitle') }}</p>
        <p class="mt-0.5 text-muted-foreground">
          {{ t('projectDetail.conflictBannerBody', { count: totalConflicts }) }}
        </p>
        <template #actions>
          <button
            class="text-xs font-medium text-warning hover:underline cursor-pointer"
            @click="openSection('conflicts')"
          >
            {{ t('projectDetail.resolve') }}
          </button>
        </template>
      </Banner>

      <ProjectOverview v-if="section === 'overview'" :project="project" @open="openSection" />
      <div v-else class="pt-4">
        <ProjectBoard v-if="section === 'board'" :project="project" />
        <!-- Session defaults (UX-19): engine, models, mode, effort, style, Claude account -->
        <ProjectSessionDefaults v-else-if="section === 'session'" :project="project" />
        <!-- This project's own response styles -->
        <div v-else-if="section === 'styles'" class="max-w-lg">
          <ResponseStylesSettings :project-id="project.id" />
        </div>
        <ProjectConnections v-else-if="place.tab === 'connections'" :project="project" :section="section as 'github'" />
        <ProjectLibrary v-else-if="place.tab === 'library'" :project="project" :section="section as 'skills'" />
        <ProjectServers v-else-if="place.tab === 'servers'" :project="project" :section="section as 'deploy'" />
      </div>
    </div>
  </div>
</template>
