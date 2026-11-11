<script setup lang="ts">
// GitHub Project (V2) board section of the project detail column: link a board
// and map its start date / deadline / status fields.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { KanbanSquare, Loader2 } from 'lucide-vue-next'
import { AppSelect, Banner, Button, Card, Input } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { useProjectsStore, type Project } from '@/stores/projects'
import { useGithubAccountsStore } from '@/stores/githubAccounts'

const props = defineProps<{ project: Project }>()

const { t } = useI18n()
const { toast } = useToast()
const projectStore = useProjectsStore()
const ghStore = useGithubAccountsStore()

if (!ghStore.accounts.length) ghStore.fetch().catch(() => {})

const linkedAccount = computed(() => ghStore.accounts.find(a => a.id === props.project.github_account_id) ?? null)

interface BoardField { id: string; name: string; dataType: string }
interface BoardInfo {
  id: string; title: string; url: string; number: number
  owner: string; ownerType: string; fields: BoardField[]
}

const boardUrl = ref('')
const boardInfo = ref<BoardInfo | null>(null)
const resolvingBoard = ref(false)
const savingBoard = ref(false)
const boardError = ref<string | null>(null)
const mapStart = ref('')
const mapDeadline = ref('')
const mapStatus = ref('')

const noneOpt = { value: '', label: '— None —' }
const dateFieldOptions = computed(() => [
  noneOpt,
  ...(boardInfo.value?.fields ?? [])
    .filter(f => f.dataType === 'DATE' || f.dataType === 'ITERATION')
    .map(f => ({ value: f.id, label: `${f.name} (${f.dataType})` })),
])
const statusFieldOptions = computed(() => [
  noneOpt,
  ...(boardInfo.value?.fields ?? [])
    .filter(f => f.dataType === 'SINGLE_SELECT')
    .map(f => ({ value: f.id, label: f.name })),
])

// Load saved config into the form when the project changes.
watch(() => props.project, (p) => {
  boardUrl.value = p.github_project_board_url ?? ''
  boardInfo.value = null
  boardError.value = null
  mapStart.value = ''
  mapDeadline.value = ''
  mapStatus.value = ''
  if (p.github_project_field_mappings) {
    try {
      const m = JSON.parse(p.github_project_field_mappings)
      mapStart.value = m.startFieldId ?? ''
      mapDeadline.value = m.deadlineFieldId ?? ''
      mapStatus.value = m.statusFieldId ?? ''
    } catch { /* ignore malformed */ }
  }
}, { immediate: true })

async function resolveBoard() {
  if (!boardUrl.value.trim()) return
  resolvingBoard.value = true
  boardError.value = null
  try {
    boardInfo.value = await projectStore.resolveProjectBoard(props.project.id, boardUrl.value.trim())
  } catch (e) {
    boardInfo.value = null
    boardError.value = String(e)
  } finally {
    resolvingBoard.value = false
  }
}

async function saveBoard() {
  if (!boardInfo.value) return
  savingBoard.value = true
  try {
    const mappings = {
      boardId: boardInfo.value.id,
      ownerType: boardInfo.value.ownerType,
      owner: boardInfo.value.owner,
      number: boardInfo.value.number,
      startFieldId: mapStart.value || null,
      deadlineFieldId: mapDeadline.value || null,
      statusFieldId: mapStatus.value || null,
    }
    await projectStore.updateProject({
      id: props.project.id,
      name: props.project.name,
      github_project_board_url: boardUrl.value.trim(),
      github_project_field_mappings: JSON.stringify(mappings),
    })
    toast.success(t('projectDetail.toastBoardSaved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    savingBoard.value = false
  }
}

async function unlinkBoard() {
  savingBoard.value = true
  try {
    await projectStore.updateProject({
      id: props.project.id,
      name: props.project.name,
      github_project_board_url: null,
      github_project_field_mappings: null,
    })
    boardUrl.value = ''
    boardInfo.value = null
    mapStart.value = ''
    mapDeadline.value = ''
    mapStatus.value = ''
    toast.success(t('projectDetail.toastBoardUnlinked'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    savingBoard.value = false
  }
}
</script>

<template>
  <div class="max-w-lg space-y-4">
    <Card body-class="p-4 space-y-4">
      <template #header>
        <KanbanSquare class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.githubProjectBoard') }}</span>
      </template>
      <p class="text-2xs text-muted-foreground leading-relaxed">
        {{ t('projectDetail.boardIntroPrefix') }}
        <code class="text-primary">read:project</code>{{ t('projectDetail.boardIntroScope') }}
      </p>

      <Banner v-if="!linkedAccount" tone="warning">
        {{ t('projectDetail.linkGithubFirst', { tab: t('projectDetail.githubTab') }) }}
      </Banner>

      <template v-else>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground">{{ t('projectDetail.boardUrl') }}</label>
          <div class="flex gap-2">
            <Input
              v-model="boardUrl"
              size="sm"
              placeholder="https://github.com/orgs/<owner>/projects/<n>"
              class="flex-1"
              :disabled="resolvingBoard || savingBoard"
              @keyup.enter="resolveBoard"
            />
            <Button variant="outline" size="sm" :disabled="resolvingBoard || !boardUrl.trim()" @click="resolveBoard">
              <Loader2 v-if="resolvingBoard" class="h-3.5 w-3.5 animate-spin" />
              <span>{{ resolvingBoard ? t('projectDetail.connecting') : t('projectDetail.connect') }}</span>
            </Button>
          </div>
        </div>

        <Banner v-if="boardError" tone="destructive">
          {{ boardError }}
        </Banner>

        <div v-if="boardInfo" class="space-y-3">
          <div class="text-2xs text-muted-foreground">
            {{ t('projectDetail.boardLabel') }} <span class="text-foreground font-medium">{{ boardInfo.title }}</span>
            · {{ t('projectDetail.fieldsCount', { count: boardInfo.fields.length }) }}
          </div>
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground">{{ t('projectDetail.startDateField') }}</label>
            <AppSelect size="sm" v-model="mapStart" :options="dateFieldOptions" />
          </div>
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground">{{ t('projectDetail.deadlineField') }}</label>
            <AppSelect size="sm" v-model="mapDeadline" :options="dateFieldOptions" />
          </div>
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground">{{ t('projectDetail.statusField') }}</label>
            <AppSelect size="sm" v-model="mapStatus" :options="statusFieldOptions" />
          </div>
          <div class="flex gap-2 pt-1">
            <Button size="sm" :disabled="savingBoard" @click="saveBoard">
              <Loader2 v-if="savingBoard" class="h-3.5 w-3.5 animate-spin" />
              <span>{{ t('projectDetail.saveConfiguration') }}</span>
            </Button>
            <Button variant="ghost" size="sm" :disabled="savingBoard" @click="unlinkBoard">{{ t('projectDetail.unlink') }}</Button>
          </div>
        </div>

        <div v-else-if="project.github_project_board_url" class="text-2xs text-muted-foreground">
          {{ t('projectDetail.savedBoardPrefix') }} <span class="text-foreground">{{ project.github_project_board_url }}</span>.
          {{ t('projectDetail.savedBoardSuffix') }}
          <div class="pt-2">
            <Button variant="ghost" size="sm" :disabled="savingBoard" @click="unlinkBoard">{{ t('projectDetail.unlink') }}</Button>
          </div>
        </div>
      </template>
    </Card>
  </div>
</template>
