<script setup lang="ts">
// "Add project" modal (path, name, repositories), shared by the Projects page and
// the titlebar project switcher. It owns the form state; the parent owns the
// detected folder and the add call.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, FolderOpen, GitBranch, Github, Gitlab, Trash2, AlertTriangle } from 'lucide-vue-next'
import { Button, Input, Badge, Modal } from '@/components/ui'
import type { DetectedProjectInfo, DetectedRepo } from '@/stores/projects'
import { parseRepoUrl, repoKey } from '@/lib/repoUrl'

const props = defineProps<{ pending: DetectedProjectInfo | null; busy: boolean }>()
const emit = defineEmits<{
  cancel: []
  confirm: [payload: { name: string; repos: DetectedRepo[] }]
}>()

const { t } = useI18n()

interface PendingRepo extends DetectedRepo {
  _key: number
}

let _keyCounter = 0

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

// A new folder resets the form.
watch(
  () => props.pending,
  (info) => {
    pendingName.value = info?.name ?? ''
    pendingRepos.value = info ? info.repos.map(r => ({ ...r, _key: _keyCounter++ })) : []
    newRepoUrl.value = ''
  },
  { immediate: true },
)

function addRepoFromUrl() {
  const parsed = parsedRepo.value
  if (!parsed || isDuplicateRepo.value) return
  pendingRepos.value.push({
    name: parsed.name,
    path: props.pending?.path ?? '',
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

function submit() {
  const repos = pendingRepos.value
    .filter(r => r.name.trim())
    .map(({ _key, ...repo }) => ({ ...repo, name: repo.name.trim() }))
  emit('confirm', { name: pendingName.value.trim(), repos })
}
</script>

<template>
  <Modal :open="!!pending" size="md" scroll-body :title="t('projects.addProject')" @close="emit('cancel')">
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
      <Button variant="ghost" @click="emit('cancel')">
        {{ t('common.cancel') }}
      </Button>
      <Button
        :disabled="busy || !pendingName.trim()"
        @click="submit"
      >
        <Plus class="h-3.5 w-3.5" :stroke-width="2" />
        {{ busy ? t('projects.adding') : t('projects.addProject') }}
      </Button>
    </template>
  </Modal>
</template>
