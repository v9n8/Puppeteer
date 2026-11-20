<script setup lang="ts">
// Projects page (UX-49): the project list on the left, the selected project on
// the right. Management only — no click here opens a session; sessions open from
// the project tabs on the titlebar (D3). `?id=` picks the project, `?tab=` its tab
// and `?section=` the section in that tab (the old settings page, UX-50).
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useProjectsStore, type DetectedProjectInfo, type DetectedRepo } from '@/stores/projects'
import { visibleProjects } from '@/lib/projectAppearance'
import { projectPlace, selectedProjectId, type ProjectPlace } from '@/lib/projectList'
import { listPaneClass } from '@/components/listDetail/rowStyles'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useAzureAccountsStore } from '@/stores/azureAccounts'
import { open } from '@tauri-apps/plugin-dialog'
import { Plus, FolderOpen } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import DetailEmpty from '@/components/listDetail/DetailEmpty.vue'
import ProjectList from '@/components/projects/ProjectList.vue'
import ProjectDetailPane from '@/components/projects/ProjectDetailPane.vue'
import AddProjectModal from '@/components/projects/AddProjectModal.vue'
import AddProjectIntro from '@/components/projects/AddProjectIntro.vue'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const store = useProjectsStore()
const ghStore = useGithubAccountsStore()
const glStore = useGitlabAccountsStore()
const awsStore = useAwsAccountsStore()
const azureStore = useAzureAccountsStore()
const tabs = useWorkspaceTabsStore()
const { toast } = useToast()
const adding = ref(false)
const detecting = ref(false)

const addLabel = computed(() =>
  detecting.value ? t('projects.detecting') : adding.value ? t('projects.adding') : t('projects.addProject'),
)

// --- List and selection ---------------------------------------------------------
// Hidden projects stay off the list until "Show hidden projects" is on; they are
// not deleted, so their sessions, links and tools keep working.
const search = ref('')
const showHidden = ref(false)
const shownProjects = computed(() => visibleProjects(store.projects, showHidden.value))
const hiddenCount = computed(() => store.projects.filter(p => p.hidden).length)

const selectedId = computed(() => selectedProjectId(store.projects, shownProjects.value, route.query.id))
const selected = computed(() => store.projects.find(p => p.id === selectedId.value) ?? null)
const place = computed<ProjectPlace>({
  get: () => projectPlace(route.query.tab, route.query.section, route.query.type),
  set: ({ tab, section, type }) => router.replace({ query: { ...route.query, tab, section: section ?? undefined, type: type ?? undefined } }),
})

function select(id: string) {
  router.replace({ query: { ...route.query, id } })
}

/** A drop in the list, by id: the store reorders the full list (hidden projects included). */
function move(fromId: string, toId: string) {
  const index = (id: string) => store.projects.findIndex(p => p.id === id)
  store.reorder(index(fromId), index(toId))
}

const listProps = computed(() => ({
  projects: shownProjects.value,
  selectedId: selectedId.value,
  hiddenCount: hiddenCount.value,
  emptyStore: !store.loading && store.projects.length === 0,
  loading: store.loading,
  error: store.error,
  addLabel: addLabel.value,
  addDisabled: adding.value || detecting.value,
}))
const emptyText = computed(() =>
  store.loading ? t('common.loading') : store.projects.length === 0 ? t('projects.emptyTitle') : t('projects.noSelection'),
)

const listEvents = { select, add: () => handleAdd(), retry: () => store.fetchProjects(), move }

const pending = ref<DetectedProjectInfo | null>(null)
// `?add=1` (sidebar Session with no project tab open): explain, pick a folder,
// and once the project is added open its Session instead of staying here.
const introOpen = ref(false)
const fromSession = ref(false)
watch(
  () => route.query.add,
  (add) => {
    if (add !== '1') return
    introOpen.value = true
    fromSession.value = true
    const rest = { ...route.query }
    delete rest.add
    router.replace({ query: rest })
  },
  { immediate: true },
)

function closeIntro() {
  introOpen.value = false
  if (!pending.value) fromSession.value = false
}

async function pickFromIntro() {
  await handleAdd()
  if (pending.value) introOpen.value = false
}

onMounted(async () => {
  await store.fetchProjects()
  if (ghStore.accounts.length === 0) ghStore.fetch()
  if (glStore.accounts.length === 0) glStore.fetch()
  if (awsStore.accounts.length === 0) awsStore.fetch()
  if (azureStore.accounts.length === 0) azureStore.fetch()
})

async function handleAdd() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: t('projects.selectFolderTitle'),
  })
  if (!selected) return
  detecting.value = true
  try {
    const info = await store.detectProjectInfo(selected)
    pending.value = info
  } catch (e) {
    toast.error(String(e))
  } finally {
    detecting.value = false
  }
}

function cancelAdd() {
  fromSession.value = false
  pending.value = null
}

async function confirmAdd({ name, repos: edited }: { name: string; repos: DetectedRepo[] }) {
  if (!pending.value) return
  adding.value = true
  try {
    const repos = edited
      .map(r => ({
        name: r.name,
        path: r.path,
        provider: r.provider,
        github_owner: r.github_owner || undefined,
        github_repo: r.github_repo || undefined,
        gitlab_project_path: r.gitlab_project_path || null,
      }))

    const project = await store.addProject({
      path: pending.value.path,
      name: name || undefined,
      repos: repos.length > 0 ? repos : undefined,
    })
    pending.value = null
    toast.success(t('projects.toastAdded'))
    if (fromSession.value) {
      fromSession.value = false
      tabs.open(project.id)
      tabs.touch(project.id)
      router.push({ name: 'project-run', params: { projectId: project.id } })
    } else {
      select(project.id)
    }
  } catch (e) {
    toast.error(String(e).includes('project_is_home') ? t('projects.errorHomeFolder') : String(e))
  } finally {
    adding.value = false
  }
}
</script>

<template>
  <div class="flex h-full min-h-0">
    <aside class="w-[240px] min-[1024px]:w-[280px]" :class="listPaneClass">
      <ProjectList v-model:search="search" v-model:show-hidden="showHidden" v-bind="listProps" v-on="listEvents" />
    </aside>

    <ProjectDetailPane v-if="selected" v-model:place="place" :project="selected" />
    <DetailEmpty v-else :icon="FolderOpen" :text="emptyText">
      <div class="mt-4 flex justify-center gap-2">
        <Button v-if="!store.loading" size="sm" :disabled="adding || detecting" @click="handleAdd">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ addLabel }}
        </Button>
      </div>
    </DetailEmpty>

    <AddProjectIntro :open="introOpen" :busy="adding || detecting" @close="closeIntro" @pick="pickFromIntro" />

    <AddProjectModal :pending="pending" :busy="adding" @cancel="cancelAdd" @change-folder="handleAdd" @confirm="confirmAdd" />
  </div>
</template>
