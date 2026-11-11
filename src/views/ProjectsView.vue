<script setup lang="ts">
// Projects page (UX-49): the project list on the left, the selected project on
// the right. Management only — no click here opens a session; sessions open from
// the project tabs on the titlebar (D3). `?id=` picks the project, `?tab=` its tab
// and `?section=` the section in that tab (the old settings page, UX-50).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useMediaQuery } from '@vueuse/core'
import { useProjectsStore, type DetectedProjectInfo, type DetectedRepo } from '@/stores/projects'
import { visibleProjects } from '@/lib/projectAppearance'
import { projectPlace, selectedProjectId, type ProjectPlace } from '@/lib/projectList'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useAzureAccountsStore } from '@/stores/azureAccounts'
import { open } from '@tauri-apps/plugin-dialog'
import { parseRepoUrl, repoKey } from '@/lib/repoUrl'
import { Plus, FolderOpen, GitBranch, Github, Gitlab, Trash2, AlertTriangle, PanelLeft } from 'lucide-vue-next'
import { Button, Input, Badge, Modal, Drawer } from '@/components/ui'
import DetailEmpty from '@/components/listDetail/DetailEmpty.vue'
import ProjectList from '@/components/projects/ProjectList.vue'
import ProjectDetailPane from '@/components/projects/ProjectDetailPane.vue'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const store = useProjectsStore()
const ghStore = useGithubAccountsStore()
const glStore = useGitlabAccountsStore()
const awsStore = useAwsAccountsStore()
const azureStore = useAzureAccountsStore()
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
  get: () => projectPlace(route.query.tab, route.query.section),
  set: ({ tab, section }) => router.replace({ query: { ...route.query, tab, section: section ?? undefined } }),
})

// On a narrow window the list leaves the row and opens as a drawer.
const narrow = useMediaQuery('(max-width: 1023px)')
const listOpen = ref(false)

function select(id: string) {
  listOpen.value = false
  router.replace({ query: { ...route.query, id } })
}

/** A drop in the list, by id: the store reorders the full list (hidden projects included). */
function move(fromId: string, toId: string) {
  const index = (id: string) => store.projects.findIndex(p => p.id === id)
  store.reorder(index(fromId), index(toId))
}

// The same list sits beside the detail or, on a narrow window, in a drawer.
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

interface PendingRepo extends DetectedRepo {
  _key: number
}

let _keyCounter = 0

const pending = ref<DetectedProjectInfo | null>(null)
const pendingName = ref('')
const pendingRepos = ref<PendingRepo[]>([])

// Adding a repo mirrors project settings: paste a link, we parse it — no manual
// provider / owner / repo fields.
const newRepoUrl = ref('')
const parsedRepo = computed(() => parseRepoUrl(newRepoUrl.value))

// URL typed but not yet a valid repo link — used to show a gentle hint.
const repoUrlInvalid = computed(() => !!newRepoUrl.value.trim() && !parsedRepo.value)

// The repo being entered is already on the list (auto-detected or just pasted).
const isDuplicateRepo = computed(() => {
  const parsed = parsedRepo.value
  if (!parsed) return false
  const key = repoKey(parsed)
  return pendingRepos.value.some(r => repoKey(r) === key)
})

const canAddRepo = computed(() => !!parsedRepo.value && !isDuplicateRepo.value)

// Second line of a repo row: its provider coordinates, or the path when the
// folder has no recognised remote.
function repoCoords(repo: PendingRepo): string {
  if (repo.provider === 'gitlab') return repo.gitlab_project_path || repo.path
  return repo.github_owner && repo.github_repo
    ? `${repo.github_owner}/${repo.github_repo}`
    : repo.path
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
    pendingName.value = info.name
    pendingRepos.value = info.repos.map(r => ({ ...r, _key: _keyCounter++ }))
    newRepoUrl.value = ''
  } catch (e) {
    toast.error(String(e))
  } finally {
    detecting.value = false
  }
}

function addRepoFromUrl() {
  const parsed = parsedRepo.value
  if (!parsed || isDuplicateRepo.value) return
  pendingRepos.value.push({
    name: parsed.name,
    path: pending.value?.path ?? '',
    provider: parsed.provider,
    github_owner: parsed.github_owner || null,
    github_repo: parsed.github_repo || null,
    gitlab_project_path: parsed.gitlab_project_path || null,
    _key: _keyCounter++,
  })
  newRepoUrl.value = ''
}

function removeRepoRow(key: number) {
  pendingRepos.value = pendingRepos.value.filter(r => r._key !== key)
}

function cancelAdd() {
  pending.value = null
  pendingRepos.value = []
  newRepoUrl.value = ''
}

async function confirmAdd() {
  if (!pending.value) return
  adding.value = true
  try {
    const repos = pendingRepos.value
      .filter(r => r.name.trim())
      .map(r => ({
        name: r.name.trim(),
        path: r.path,
        provider: r.provider,
        github_owner: r.github_owner || undefined,
        github_repo: r.github_repo || undefined,
        gitlab_project_path: r.gitlab_project_path || null,
      }))

    const project = await store.addProject({
      path: pending.value.path,
      name: pendingName.value || undefined,
      repos: repos.length > 0 ? repos : undefined,
    })
    pending.value = null
    pendingRepos.value = []
    newRepoUrl.value = ''
    toast.success(t('projects.toastAdded'))
    select(project.id)
  } catch (e) {
    toast.error(String(e).includes('project_is_home') ? t('projects.errorHomeFolder') : String(e))
  } finally {
    adding.value = false
  }
}
</script>

<template>
  <div class="flex h-full min-h-0">
    <aside v-if="!narrow" class="flex w-[280px] shrink-0 flex-col border-r border-border/60">
      <ProjectList v-model:search="search" v-model:show-hidden="showHidden" v-bind="listProps" v-on="listEvents" />
    </aside>
    <Drawer v-else :open="listOpen" side="left" size="sm" :title="t('projects.title')" @close="listOpen = false">
      <ProjectList v-model:search="search" v-model:show-hidden="showHidden" v-bind="listProps" v-on="listEvents" />
    </Drawer>

    <ProjectDetailPane v-if="selected" v-model:place="place" :project="selected">
      <template v-if="narrow" #leading>
        <Button
          variant="outline"
          size="icon"
          class="shrink-0"
          :title="t('projects.showList')"
          :aria-label="t('projects.showList')"
          @click="listOpen = true"
        >
          <PanelLeft class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
      </template>
    </ProjectDetailPane>
    <DetailEmpty v-else :icon="FolderOpen" :text="emptyText">
      <div class="mt-4 flex justify-center gap-2">
        <Button v-if="narrow && store.projects.length > 0" variant="outline" size="sm" @click="listOpen = true">
          <PanelLeft class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ t('projects.showList') }}
        </Button>
        <Button v-if="!store.loading" size="sm" :disabled="adding || detecting" @click="handleAdd">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ addLabel }}
        </Button>
      </div>
    </DetailEmpty>

    <!-- Add project modal -->
    <Modal :open="!!pending" size="md" scroll-body :title="t('projects.addProject')" @close="cancelAdd">
      <template v-if="pending">
          <div class="px-5 py-5 flex flex-col gap-4">
            <!-- Path (read-only) -->
            <div>
              <label class="block text-xs text-muted-foreground mb-1.5">{{ t('projects.path') }}</label>
              <div class="flex items-center gap-2 px-3 py-2 bg-muted/50 border border-border/60 rounded-md">
                <FolderOpen class="h-3.5 w-3.5 text-muted-foreground shrink-0" :stroke-width="1.5" />
                <span class="text-xs font-mono text-muted-foreground truncate">{{ pending.path }}</span>
              </div>
            </div>

            <!-- Name -->
            <div>
              <label class="block text-xs text-muted-foreground mb-1.5">{{ t('projects.projectName') }}</label>
              <Input
                v-model="pendingName"
                :placeholder="t('projects.projectNamePlaceholder')"
              />
            </div>

            <!-- Repos — same paste-a-URL flow as project settings -->
            <div>
              <div class="flex items-center gap-1.5 mb-2">
                <label class="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <GitBranch class="h-3 w-3" :stroke-width="1.5" />
                  {{ t('projects.repositories') }}
                </label>
                <span
                  v-if="pendingRepos.length"
                  class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
                >{{ pendingRepos.length }}</span>
                <span v-if="pending.repos.length > 0" class="text-2xs text-success font-medium">
                  {{ t('projects.autoDetected', { count: pending.repos.length }) }}
                </span>
              </div>

              <!-- Add new repo — paste a URL, no manual fields -->
              <div class="space-y-2">
                <div class="flex gap-2">
                  <div class="relative flex-1">
                    <GitBranch class="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground pointer-events-none" :stroke-width="1.5" />
                    <Input
                      v-model="newRepoUrl"
                      type="text"
                      size="sm"
                      class="pl-8"
                      :placeholder="t('projects.repoUrlPlaceholder')"
                      @keydown.enter="addRepoFromUrl"
                    />
                  </div>
                  <Button
                    size="sm"
                    :disabled="!canAddRepo"
                    @click="addRepoFromUrl"
                  >
                    <Plus class="h-3.5 w-3.5" :stroke-width="2" />
                    {{ t('common.add') }}
                  </Button>
                </div>

                <!-- Parsed preview: what will be added -->
                <div
                  v-if="parsedRepo"
                  class="flex items-center gap-1.5 flex-wrap text-2xs px-0.5"
                >
                  <Badge :tone="parsedRepo.provider === 'gitlab' ? 'warning' : 'info'" size="xs">
                    <component :is="parsedRepo.provider === 'gitlab' ? Gitlab : Github" class="h-2.5 w-2.5" :stroke-width="2" />
                    {{ parsedRepo.provider === 'gitlab' ? 'GitLab' : 'GitHub' }}
                  </Badge>
                  <span class="font-mono text-muted-foreground truncate">
                    {{ parsedRepo.provider === 'gitlab' ? parsedRepo.gitlab_project_path : `${parsedRepo.github_owner}/${parsedRepo.github_repo}` }}
                  </span>
                  <span v-if="isDuplicateRepo" class="flex items-center gap-1 text-warning font-medium">
                    <AlertTriangle class="h-2.5 w-2.5" :stroke-width="2" />
                    {{ t('projects.alreadyAdded') }}
                  </span>
                </div>

                <!-- Invalid URL hint -->
                <p v-else-if="repoUrlInvalid" class="text-2xs text-muted-foreground px-0.5">
                  {{ t('projects.repoUrlHint') }}
                </p>
              </div>

              <!-- Repos to create with the project (remove to change) -->
              <div v-if="pendingRepos.length" class="border-t border-border/60 pt-3 mt-3 space-y-1.5">
                <div
                  v-for="repo in pendingRepos"
                  :key="repo._key"
                  class="flex items-center gap-2 border border-border rounded-md px-3 py-2"
                >
                  <component
                    :is="repo.provider === 'gitlab' ? Gitlab : Github"
                    class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
                    :stroke-width="1.75"
                  />
                  <div class="min-w-0 flex-1">
                    <p class="text-xs font-medium truncate">{{ repo.name }}</p>
                    <p class="text-2xs text-muted-foreground font-mono truncate">{{ repoCoords(repo) }}</p>
                  </div>
                  <Button
                    variant="destructive-ghost"
                    size="icon-sm"
                    class="shrink-0"
                    :title="t('projects.removeRepo')"
                    @click="removeRepoRow(repo._key)"
                  >
                    <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
                  </Button>
                </div>
              </div>

              <p v-else class="text-2xs text-muted-foreground text-center py-2">
                {{ t('projects.noReposHint') }}
              </p>
            </div>
          </div>

      </template>

      <template #footer>
        <Button variant="ghost" @click="cancelAdd">
          {{ t('common.cancel') }}
        </Button>
        <Button
          :disabled="adding || !pendingName.trim()"
          @click="confirmAdd"
        >
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ adding ? t('projects.adding') : t('projects.addProject') }}
        </Button>
      </template>
    </Modal>
  </div>
</template>
