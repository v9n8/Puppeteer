<script setup lang="ts">
// Library (UX-54, D1/D8/D9): the skills, rules, commands and agents the CLIs
// read, straight from the disk (commands/library_disk.rs) — Global is HOME's
// folders, a project's tab its own. Replaces the Skills, Rules, Agents,
// Commands and Import screens; nothing is created here, only listed, edited,
// deleted, or duplicated from a project to Global.
//
// The place lives in the URL query (lib/libraryDisk `libraryQuery`), so a link
// can open a project's tab or one file.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { onBeforeRouteLeave, useRoute, useRouter } from 'vue-router'
import { AlertCircle, Library, RotateCw } from 'lucide-vue-next'
import { Badge, Button, PageHeader, SegmentedControl } from '@/components/ui'
import ListDetailSidebar from '@/components/listDetail/ListDetailSidebar.vue'
import DetailEmpty from '@/components/listDetail/DetailEmpty.vue'
import LibraryProjectList from '@/components/library/LibraryProjectList.vue'
import LibraryEntryDetail from '@/components/library/LibraryEntryDetail.vue'
import { invoke } from '@/lib/tauri'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useProjectsStore } from '@/stores/projects'
import { selectedProjectId } from '@/lib/projectList'
import {
  CMD_LIBRARY_DELETE, CMD_LIBRARY_DUPLICATE, CMD_LIBRARY_SCAN, deletesLink, isLibraryExists, kindGroups,
  libraryPlace, libraryQuery, type DiskFile, type DiskItem, type LibraryPlace, type LibraryScope,
} from '@/lib/libraryDisk'

defineOptions({ name: 'LibraryView' })

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const projectsStore = useProjectsStore()
const { confirm } = useConfirm()
const { toast } = useToast()

const scope = ref<LibraryScope>('global')
/** The project tab's pick; kept while on Global so coming back returns to it. */
const projectId = ref<string | null>(null)
const selectedPath = ref<string | null>(null)

const visibleProjects = computed(() => projectsStore.projects.filter((p) => !p.hidden))
/** The project whose items are listed: the picked one (a hidden one too, from a link), else the first. */
const activeProjectId = computed(() =>
  scope.value === 'project' ? selectedProjectId(projectsStore.projects, visibleProjects.value, projectId.value) : null,
)

const scopeOptions = computed(() => [
  { value: 'global' as const, label: t('library.tabGlobal') },
  { value: 'project' as const, label: t('library.tabProject') },
])

const COLLAPSED_KEY = 'puppeteer.library.projectsCollapsed'
const projectsCollapsed = ref(readCollapsed())

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(COLLAPSED_KEY) === '1'
  } catch {
    return false
  }
}

watch(projectsCollapsed, (v) => {
  try {
    localStorage.setItem(COLLAPSED_KEY, v ? '1' : '0')
  } catch {
    /* storage unavailable */
  }
})

// ── Items ───────────────────────────────────────────────────────────────────

const items = ref<DiskItem[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const search = ref('')
// None while the first scan runs, so the list shows its skeleton rather than four empty groups.
const groups = computed(() =>
  loading.value && items.value.length === 0 ? [] : kindGroups(items.value, search.value, (k) => t(`library.kind.${k}`)),
)
const selectedItem = computed(() => items.value.find((i) => i.path === selectedPath.value) ?? null)
const detailRef = ref<InstanceType<typeof LibraryEntryDetail> | null>(null)

let scanSeq = 0
async function scan() {
  const seq = ++scanSeq
  const pid = activeProjectId.value
  if (scope.value === 'project' && !pid) {
    items.value = []
    error.value = null
    return
  }
  loading.value = true
  try {
    const list = await invoke<DiskItem[]>(CMD_LIBRARY_SCAN, { scope: scope.value, projectId: pid })
    if (seq !== scanSeq) return
    items.value = list
    error.value = null
    // Gone from the disk (deleted elsewhere, a stale link): drop the selection.
    if (selectedPath.value && !list.some((i) => i.path === selectedPath.value)) setPlace({ path: null })
  } catch (e) {
    if (seq !== scanSeq) return
    items.value = []
    error.value = String(e)
  } finally {
    if (seq === scanSeq) loading.value = false
  }
}

watch([scope, activeProjectId], () => void scan())

async function refresh() {
  await Promise.all([scan(), scope.value === 'project' ? projectsStore.fetchProjects() : null])
}

onMounted(() => {
  void projectsStore.fetchProjects()
  void scan()
})

// ── Place (URL query) ───────────────────────────────────────────────────────

function current(): LibraryPlace {
  return { scope: scope.value, projectId: activeProjectId.value, path: selectedPath.value }
}

function setPlace(next: Partial<LibraryPlace>) {
  if (next.scope !== undefined) scope.value = next.scope
  if (next.projectId !== undefined) projectId.value = next.projectId
  if (next.path !== undefined) selectedPath.value = next.path
  void router.replace({ query: libraryQuery(current()) })
}

/** Leaving unsaved edits takes a confirm; true = go ahead. */
async function confirmDiscard(): Promise<boolean> {
  if (!detailRef.value?.dirty) return true
  return confirm({
    title: t('library.discardTitle'),
    message: t('library.discardMessage'),
    confirmLabel: t('library.discard'),
  })
}

async function go(next: Partial<LibraryPlace>) {
  if (!(await confirmDiscard())) return
  setPlace(next)
}

function selectScope(next: LibraryScope) {
  if (next !== scope.value) void go({ scope: next, path: null })
}

function selectProject(id: string) {
  if (id !== activeProjectId.value) void go({ projectId: id, path: null })
}

function selectItem(path: string) {
  if (path !== selectedPath.value) void go({ path })
}

// A link (or Back) asks for another place: unsaved edits are confirmed, declining restores the URL.
watch(
  () => route.query,
  async (query) => {
    if (route.path !== '/library') return
    const want = libraryPlace(query)
    const same =
      want.scope === scope.value &&
      want.path === selectedPath.value &&
      (want.scope === 'global' || want.projectId === null || want.projectId === activeProjectId.value)
    if (same) return
    if (!(await confirmDiscard())) {
      void router.replace({ query: libraryQuery(current()) })
      return
    }
    scope.value = want.scope
    if (want.projectId) projectId.value = want.projectId
    selectedPath.value = want.path
  },
  { immediate: true },
)

onBeforeRouteLeave(() => confirmDiscard())

// ── Actions ─────────────────────────────────────────────────────────────────

const deleting = ref(false)
const duplicating = ref(false)

function onSaved(file: DiskFile) {
  const it = items.value.find((i) => i.path === file.path)
  if (it) it.hash = file.hash
}

async function remove(item: DiskItem) {
  const link = deletesLink(item)
  const message = link
    ? t('library.deleteLinkMessage', { path: item.entry, target: item.link_target ?? '' })
    : t(item.kind === 'skill' ? 'library.deleteSkillMessage' : 'library.deleteFileMessage', { path: item.entry })
  const ok = await confirm({
    title: t('library.deleteTitle', { name: item.name }),
    message,
    confirmLabel: link ? t('library.removeLink') : t('library.moveToTrash'),
  })
  if (!ok) return
  deleting.value = true
  try {
    await invoke(CMD_LIBRARY_DELETE, { path: item.path })
    toast.success(link ? t('library.linkRemoved') : t('library.movedToTrash'))
    // Its unsaved edits go with it: the delete was confirmed.
    setPlace({ path: null })
    await scan()
  } catch (e) {
    toast.error(String(e))
  } finally {
    deleting.value = false
  }
}

async function duplicate(item: DiskItem) {
  const run = (overwrite: boolean) => invoke<DiskItem>(CMD_LIBRARY_DUPLICATE, { path: item.path, overwrite })
  duplicating.value = true
  try {
    let copy: DiskItem
    try {
      copy = await run(false)
    } catch (e) {
      if (!isLibraryExists(e)) throw e
      const ok = await confirm({
        title: t('library.duplicateExistsTitle'),
        message: t('library.duplicateExistsMessage', { name: item.name }),
        confirmLabel: t('library.overwrite'),
      })
      if (!ok) return
      copy = await run(true)
    }
    toast.success(t('library.duplicated', { path: copy.path }))
  } catch (e) {
    toast.error(String(e))
  } finally {
    duplicating.value = false
  }
}

const emptyTitle = computed(() =>
  scope.value === 'project' && !activeProjectId.value ? t('library.noProjects') : t('library.emptyTitle'),
)
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden">
    <PageHeader :title="t('library.title')">
      <template #meta>
        <span class="truncate text-2xs text-muted-foreground" :title="t('library.subtitle')">{{ t('library.subtitle') }}</span>
      </template>
      <template #actions>
        <SegmentedControl :model-value="scope" :options="scopeOptions" :stretch="false" @update:model-value="selectScope" />
        <Button
          variant="ghost"
          size="icon"
          :disabled="loading"
          :title="t('common.refresh')"
          :aria-label="t('common.refresh')"
          @click="refresh"
        >
          <RotateCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" :stroke-width="1.75" />
        </Button>
      </template>
    </PageHeader>

    <div class="flex min-h-0 flex-1 overflow-hidden">
      <LibraryProjectList
        v-if="scope === 'project'"
        v-model:collapsed="projectsCollapsed"
        :projects="visibleProjects"
        :selected-id="activeProjectId"
        :loading="projectsStore.loading"
        @select="selectProject"
      />

      <ListDetailSidebar
        v-model:search="search"
        :groups="groups"
        :selected-id="selectedPath"
        collapse-key="puppeteer.library.collapsed"
        :search-placeholder="t('library.searchPlaceholder')"
        :loading="loading"
        :error="error"
        :empty-store="!loading && items.length === 0"
        :empty-title="emptyTitle"
        :empty-hint="activeProjectId || scope === 'global' ? t('library.emptyHint') : ''"
        :flat-when-single="false"
        :addable="false"
        @select="selectItem"
        @retry="refresh"
      >
        <template #row="{ row, selected }">
          <span class="flex min-w-0 items-center gap-1.5">
            <span
              class="min-w-0 flex-1 truncate font-mono text-sm font-medium leading-snug"
              :class="selected && 'text-foreground'"
              :title="row.item.name"
            >
              {{ row.item.name }}
            </span>
            <span v-if="row.item.error" class="shrink-0 text-destructive" :title="row.item.error">
              <AlertCircle class="h-3.5 w-3.5" :stroke-width="1.75" />
            </span>
            <Badge
              v-if="row.item.status !== 'file'"
              size="xs"
              :tone="row.item.status === 'puppeteer_link' ? 'warning' : 'neutral'"
              class="shrink-0"
              :title="t(`library.statusHint.${row.item.status}`)"
            >
              {{ t(`library.status.${row.item.status}`) }}
            </Badge>
            <Badge size="xs" class="shrink-0">{{ t(`library.engine.${row.item.engine}`) }}</Badge>
          </span>
          <span
            v-if="row.item.description"
            class="mt-1 truncate text-2xs text-muted-foreground/70"
            :title="row.item.description"
          >
            {{ row.item.description }}
          </span>
        </template>
      </ListDetailSidebar>

      <LibraryEntryDetail
        v-if="selectedItem"
        ref="detailRef"
        :key="selectedItem.path"
        :item="selectedItem"
        :can-duplicate="scope === 'project'"
        :deleting="deleting"
        :duplicating="duplicating"
        @saved="onSaved"
        @delete="selectedItem && remove(selectedItem)"
        @duplicate="selectedItem && duplicate(selectedItem)"
      />
      <DetailEmpty v-else :icon="Library" :text="t('library.noSelection')" />
    </div>
  </div>
</template>
