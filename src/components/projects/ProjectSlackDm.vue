<script setup lang="ts">
// Reading Slack direct-message threads (#9, RT-27; `slack_threads::dm`): one switch for
// every project (off by default) and this project's own, which can only narrow it. Both
// are local-only for a remote controller, so a remote window shows the error the call returns.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { MessageSquareLock } from 'lucide-vue-next'
import { Card, ToggleSwitch } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { invoke } from '@/lib/tauri'
import { useProjectsStore, type Project } from '@/stores/projects'

const props = defineProps<{ project: Project }>()

const { t } = useI18n()
const { toast } = useToast()
const projects = useProjectsStore()

const globalOn = ref(false)
const projectOn = ref(props.project.slack_dm_read !== false)
const saving = ref<'global' | 'project' | null>(null)

watch(() => props.project.slack_dm_read, (v) => { projectOn.value = v !== false })

async function loadGlobal() {
  try {
    globalOn.value = await invoke<boolean>('get_slack_dm_read')
  } catch {
    globalOn.value = false
  }
}
void loadGlobal()

async function toggleGlobal() {
  const next = !globalOn.value
  saving.value = 'global'
  try {
    await invoke('set_slack_dm_read', { on: next })
    globalOn.value = next
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    saving.value = null
  }
}

async function toggleProject() {
  const next = !projectOn.value
  saving.value = 'project'
  try {
    await invoke<Project>('set_project_slack_dm_read', { projectId: props.project.id, on: next })
    projectOn.value = next
    await projects.fetchProjects()
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    saving.value = null
  }
}
</script>

<template>
  <Card>
    <template #header>
      <MessageSquareLock class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('projectDetail.slackDm.title') }}</span>
    </template>
    <div class="divide-y divide-border/50">
      <div class="flex items-center gap-3 px-4 py-3">
        <div class="min-w-0 flex-1">
          <p class="text-xs font-medium">{{ t('projectDetail.slackDm.global') }}</p>
          <p class="mt-0.5 text-2xs text-muted-foreground">{{ t('projectDetail.slackDm.globalHint') }}</p>
        </div>
        <ToggleSwitch
          :checked="globalOn"
          :disabled="saving !== null"
          :loading="saving === 'global'"
          :label="t('projectDetail.slackDm.global')"
          @toggle="toggleGlobal"
        />
      </div>
      <div class="flex items-center gap-3 px-4 py-3">
        <div class="min-w-0 flex-1">
          <p class="text-xs font-medium" :class="!globalOn && 'text-muted-foreground'">{{ t('projectDetail.slackDm.project') }}</p>
          <p class="mt-0.5 text-2xs text-muted-foreground">
            {{ globalOn ? t('projectDetail.slackDm.projectHint') : t('projectDetail.slackDm.projectOffHint') }}
          </p>
        </div>
        <ToggleSwitch
          :checked="globalOn && projectOn"
          :disabled="saving !== null || !globalOn"
          :loading="saving === 'project'"
          :label="t('projectDetail.slackDm.project')"
          @toggle="toggleProject"
        />
      </div>
    </div>
  </Card>
</template>
