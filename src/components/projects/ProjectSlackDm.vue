<script setup lang="ts">
// Reading Slack direct-message threads for this project (#9, RT-27; `slack_threads::dm`).
// Each project opts in on its own (off by default). The switch for every project lives
// on the Slack MCP server's detail; while it is off this one shows off and locked, and
// the stored opt-in comes back once it is on. Shown only while a Slack MCP server is on
// for the project (ProjectMcp). Local-only for a remote controller, so a remote window
// shows the error the call returns.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { MessageSquareLock } from 'lucide-vue-next'
import { Card, ToggleSwitch } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { invoke } from '@/lib/tauri'
import { parentLock, switchView } from '@/lib/configLock'
import { useProjectsStore, type Project } from '@/stores/projects'

const props = defineProps<{ project: Project }>()

const { t } = useI18n()
const { toast } = useToast()
const projects = useProjectsStore()

/** null until loaded: the switch waits instead of flashing locked. */
const globalOn = ref<boolean | null>(null)
const projectOn = ref(props.project.slack_dm_read === true)
const saving = ref(false)

watch(() => props.project.slack_dm_read, (v) => { projectOn.value = v === true })

const view = computed(() => switchView(projectOn.value, parentLock(globalOn.value !== false)))

async function loadGlobal() {
  try {
    globalOn.value = await invoke<boolean>('get_slack_dm_read')
  } catch (e) {
    console.warn('[slackDm] get_slack_dm_read failed', e)
    globalOn.value = false
  }
}
void loadGlobal()

async function toggleProject() {
  const next = !projectOn.value
  saving.value = true
  try {
    await invoke<Project>('set_project_slack_dm_read', { projectId: props.project.id, on: next })
    projectOn.value = next
    await projects.fetchProjects()
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <Card>
    <template #header>
      <MessageSquareLock class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('projectDetail.slackDm.title') }}</span>
    </template>
    <div class="flex items-center gap-3 px-4 py-3">
      <div class="min-w-0 flex-1">
        <p class="text-xs font-medium" :class="view.lockedBy && 'text-muted-foreground'">{{ t('projectDetail.slackDm.project') }}</p>
        <p class="mt-0.5 text-2xs text-muted-foreground">
          {{ view.lockedBy ? t('projectDetail.slackDm.lockedGlobal') : t('projectDetail.slackDm.projectHint') }}
        </p>
      </div>
      <ToggleSwitch
        :checked="view.checked"
        :disabled="saving || view.disabled || globalOn === null"
        :loading="saving"
        :label="view.lockedBy ? t('projectDetail.slackDm.lockedGlobal') : t('projectDetail.slackDm.project')"
        @toggle="toggleProject"
      />
    </div>
  </Card>
</template>
