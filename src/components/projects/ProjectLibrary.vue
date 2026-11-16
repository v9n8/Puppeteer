<script setup lang="ts">
// Library tab of the project detail column: what the project has on disk
// (skills, rules, commands, agents), read-only with links into the Library
// screen that edits them (UX-57), and the old library's skill/rule sync conflicts
// to resolve.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertCircle, GitMerge, Library } from 'lucide-vue-next'
import { Badge, Banner, Button, Card } from '@/components/ui'
import LibraryKindTabs from '@/components/library/LibraryKindTabs.vue'
import { invoke } from '@/lib/tauri'
import { CMD_LIBRARY_SCAN, DISK_ITEM_KINDS, libraryQuery, type DiskItem, type LibraryItemKind } from '@/lib/libraryDisk'
import { kindCounts, pickTab, tabRows } from '@/lib/libraryTabs'
import { useProjectsStore, type InstructionWarning, type Project } from '@/stores/projects'

const props = defineProps<{ project: Project; section: 'files' | 'conflicts' }>()

const { t } = useI18n()
const projectStore = useProjectsStore()

const projectId = computed(() => props.project.id)

const projectConflicts = computed(() => projectStore.conflicts.filter(c => c.project_id === projectId.value))
const projectRuleConflicts = computed(() => projectStore.ruleConflicts.filter(c => c.project_id === projectId.value))

// --- Files on disk (the Library screen's project tab, listed only) ---
const files = ref<DiskItem[]>([])
const loadingFiles = ref(false)
const filesError = ref<string | null>(null)
const kind = ref<LibraryItemKind>('skill')
const counts = computed(() => kindCounts(files.value, DISK_ITEM_KINDS))
const rows = computed(() => tabRows(files.value, kind.value, ''))
watch(files, () => { kind.value = pickTab(files.value, DISK_ITEM_KINDS, null, kind.value) })

async function loadFiles() {
  const id = projectId.value
  loadingFiles.value = true
  try {
    const list = await invoke<DiskItem[]>(CMD_LIBRARY_SCAN, { scope: 'project', projectId: id })
    if (id !== projectId.value) return
    files.value = list
    filesError.value = null
  } catch (e) {
    if (id !== projectId.value) return
    files.value = []
    filesError.value = String(e)
  } finally {
    loadingFiles.value = false
  }
}

/** The Library screen on this project's tab, at one file when given. */
function libraryLink(path: string | null = null) {
  return { path: '/library', query: libraryQuery({ scope: 'project', projectId: projectId.value, path }) }
}

// How Claude/Codex will load this project's instruction files — warnings only, never auto-fixed.
const instructionWarnings = ref<InstructionWarning[]>([])
async function loadInstructionWarnings() {
  const id = projectId.value
  const warnings = await projectStore.getInstructionWarnings(id).catch(() => [])
  if (id === projectId.value) instructionWarnings.value = warnings
}
function instructionWarningText(w: InstructionWarning): string {
  switch (w.code) {
    case 'agents_md_too_large':
      return t('projectDetail.warnAgentsMdTooLarge', { size: Math.ceil(w.size / 1024), limit: Math.floor(w.limit / 1024) })
    case 'claude_reads_agents_md':
      return t('projectDetail.warnClaudeReadsAgentsMd')
    case 'claude_md_imports_agents_md':
      return t('projectDetail.warnClaudeMdImportsAgentsMd', { path: w.path })
    case 'codex_reads_claude_md':
      return t('projectDetail.warnCodexReadsClaudeMd', { config: w.config })
    case 'links_not_ignored':
      return t('projectDetail.warnLinksNotIgnored', { dir: w.dir })
    case 'legacy_copies_backed_up':
      return t('projectDetail.warnLegacyCopiesBackedUp', { dir: w.dir, count: w.batches })
    case 'tracked_by_git':
      return t('projectDetail.warnTrackedByGit', { paths: w.paths.join(', ') })
  }
}

// The whole tab loads once per project, so moving between its sections reads nothing again.
// Each load drops an answer for a project the user already left.
watch(projectId, async () => {
  await loadFiles()
  await loadInstructionWarnings()
}, { immediate: true })
</script>

<template>
  <!-- Files on disk: listed here, edited on the Library screen -->
  <div v-if="section === 'files'" class="max-w-lg space-y-5">
    <Banner v-if="instructionWarnings.length > 0" tone="warning">
      <p class="font-medium text-warning">{{ t('projectDetail.instructionWarningsTitle') }}</p>
      <p
        v-for="(w, i) in instructionWarnings"
        :key="i"
        class="mt-1 leading-relaxed"
      >{{ instructionWarningText(w) }}</p>
    </Banner>
    <Card>
      <template #header>
        <Library class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('library.subtitle') }}</span>
        <span
          v-if="files.length > 0"
          class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
        >{{ files.length }}</span>
        <RouterLink
          :to="libraryLink()"
          class="ml-auto shrink-0 text-2xs text-primary hover:underline"
          :title="t('projectDetail.openInLibraryHint')"
        >{{ t('projectDetail.openInLibrary') }}</RouterLink>
      </template>

      <!-- Loading -->
      <div v-if="loadingFiles" class="divide-y divide-border/50">
        <div v-for="i in 2" :key="i" class="flex items-center gap-3 px-4 py-3">
          <div class="flex-1 space-y-1.5">
            <div class="h-2.5 w-24 bg-muted animate-pulse rounded" />
            <div class="h-2 w-36 bg-muted animate-pulse rounded" />
          </div>
        </div>
      </div>

      <Banner v-else-if="filesError" tone="destructive" class="m-3">
        <span class="break-words">{{ filesError }}</span>
        <template #actions>
          <Button variant="ghost" size="xs" @click="loadFiles">{{ t('projectDetail.libraryRetry') }}</Button>
        </template>
      </Banner>

      <div
        v-else-if="files.length === 0"
        class="px-4 py-6 text-center text-xs text-muted-foreground"
      >
        {{ t('projectDetail.libraryFilesEmpty') }}
      </div>

      <!-- Tabs by kind; a row opens that file in the Library -->
      <template v-else>
        <div class="px-4 pt-2">
          <LibraryKindTabs v-model="kind" :kinds="DISK_ITEM_KINDS" :counts="counts" />
        </div>
        <p v-if="rows.length === 0" class="px-4 py-6 text-center text-xs text-muted-foreground">
          {{ t('projectDetail.libraryKindEmpty', { kind: t(`library.kindOne.${kind}`) }) }}
        </p>
        <div v-else class="divide-y divide-border/60">
          <RouterLink
            v-for="row in rows"
            :key="row.id"
            :to="libraryLink(row.item.path)"
            class="block px-4 py-2 transition-colors hover:bg-accent/50"
            :title="t('projectDetail.openFileInLibrary', { name: row.item.name })"
          >
            <span class="flex min-w-0 items-center gap-1.5">
              <span class="min-w-0 truncate font-mono text-xs font-medium" :title="row.item.name">{{ row.item.name }}</span>
              <span v-if="row.item.error" class="shrink-0 text-destructive" :title="row.item.error">
                <AlertCircle class="h-3.5 w-3.5" :stroke-width="1.75" />
              </span>
              <Badge
                v-if="row.item.status !== 'file'"
                size="xs"
                :tone="row.item.status === 'puppeteer_link' ? 'warning' : 'neutral'"
                class="shrink-0"
                :title="t(`library.statusHint.${row.item.status}`)"
              >{{ t(`library.status.${row.item.status}`) }}</Badge>
              <Badge size="xs" class="shrink-0">{{ t(`library.engine.${row.item.engine}`) }}</Badge>
            </span>
            <span
              v-if="row.item.description"
              class="mt-0.5 block truncate text-2xs text-muted-foreground"
              :title="row.item.description"
            >{{ row.item.description }}</span>
          </RouterLink>
        </div>
      </template>
    </Card>
  </div>

  <!-- Conflicts -->
  <div v-else class="max-w-lg space-y-3">
    <!-- Skill conflicts -->
    <Card
      v-for="conflict in projectConflicts"
      :key="conflict.id"
      class="border-warning/20"
      bodyClass="p-4"
    >
      <div class="flex items-start gap-3 mb-3">
        <GitMerge class="h-4 w-4 text-warning mt-0.5 shrink-0" :stroke-width="1.75" />
        <div>
          <div class="flex items-center gap-1.5">
            <p class="text-sm font-mono font-medium">{{ conflict.skill_name }}</p>
            <Badge tone="neutral" size="xs" class="uppercase tracking-wide">
              {{ t('projectDetail.conflictSkillBadge', { path: conflict.engine === 'codex' ? '.codex/skills' : '.claude/skills' }) }}
            </Badge>
          </div>
          <p class="text-xs text-muted-foreground mt-0.5">
            {{ t('projectDetail.localChangesDetected', { date: new Date(conflict.detected_at).toLocaleDateString() }) }}
          </p>
        </div>
      </div>
      <div class="flex gap-2">
        <Button class="flex-1" @click="projectStore.resolveConflict(conflict.id, true); projectStore.fetchConflicts()">
          {{ t('projectDetail.overwriteWithCentral') }}
        </Button>
        <Button variant="outline" class="flex-1" @click="projectStore.resolveConflict(conflict.id, false); projectStore.fetchConflicts()">
          {{ t('projectDetail.keepLocal') }}
        </Button>
      </div>
    </Card>

    <!-- Rule conflicts -->
    <Card
      v-for="conflict in projectRuleConflicts"
      :key="conflict.id"
      class="border-warning/20"
      bodyClass="p-4"
    >
      <div class="flex items-start gap-3 mb-3">
        <GitMerge class="h-4 w-4 text-warning mt-0.5 shrink-0" :stroke-width="1.75" />
        <div>
          <div class="flex items-center gap-1.5">
            <p class="text-sm font-mono font-medium">{{ conflict.rule_name }}</p>
            <Badge tone="neutral" size="xs" class="uppercase tracking-wide">
              {{ t('projectDetail.conflictRuleBadge', { path: conflict.engine === 'claude' ? '.claude/rules' : 'AGENTS.md' }) }}
            </Badge>
          </div>
          <p class="text-xs text-muted-foreground mt-0.5">
            {{ t('projectDetail.localChangesDetected', { date: new Date(conflict.detected_at).toLocaleDateString() }) }}
          </p>
        </div>
      </div>
      <div class="flex gap-2">
        <Button class="flex-1" @click="projectStore.resolveRuleConflict(conflict.id, true)">
          {{ t('projectDetail.overwriteWithCentral') }}
        </Button>
        <Button variant="outline" class="flex-1" @click="projectStore.resolveRuleConflict(conflict.id, false)">
          {{ t('projectDetail.keepLocal') }}
        </Button>
      </div>
    </Card>
  </div>
</template>
