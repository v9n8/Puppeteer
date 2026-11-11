<script setup lang="ts">
// Connections tab of the project detail column: the GitHub, GitLab, AWS and
// Azure accounts the project may use (several each, one default).
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import ProjectAccountsList, { type AccountRow } from '@/components/ProjectAccountsList.vue'
import { useToast } from '@/composables/useToast'
import { useProjectsStore, type Project } from '@/stores/projects'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useAzureAccountsStore } from '@/stores/azureAccounts'

const props = defineProps<{ project: Project; section: 'github' | 'gitlab' | 'aws' | 'azure' }>()

const { t } = useI18n()
const { toast } = useToast()
const projectStore = useProjectsStore()
const ghStore = useGithubAccountsStore()
const glStore = useGitlabAccountsStore()
const awsStore = useAwsAccountsStore()
const azureStore = useAzureAccountsStore()

if (!ghStore.accounts.length) ghStore.fetch().catch(() => {})
if (!glStore.accounts.length) glStore.fetch().catch(() => {})
if (!awsStore.accounts.length) awsStore.fetch().catch(() => {})
if (!azureStore.accounts.length) azureStore.fetch().catch(() => {})

// --- GitHub / GitLab accounts (several per project, toggled like servers) ---
const githubAccountRows = computed<AccountRow[]>(() => ghStore.accounts.map(a => ({
  id: a.id,
  label: a.label,
  username: a.username,
  detail: a.scopes.length ? a.scopes.join(', ') : null,
})))
const gitlabAccountRows = computed<AccountRow[]>(() => glStore.accounts.map(a => ({
  id: a.id,
  label: a.label,
  username: a.username,
  detail: a.host,
})))
const busyGitAccountId = ref<string | null>(null)

async function handleToggleGitAccount(provider: 'github' | 'gitlab', account: AccountRow, enabled: boolean) {
  if (busyGitAccountId.value) return
  busyGitAccountId.value = account.id
  try {
    await projectStore.setProjectGitAccountEnabled(props.project.id, provider, account.id, enabled)
    toast.success(t(enabled ? 'projectDetail.toastAccountEnabled' : 'projectDetail.toastAccountDisabled', { name: account.label }))
  } catch (e) {
    toast.error(String(e))
  } finally {
    busyGitAccountId.value = null
  }
}

async function handleMakeDefaultGitAccount(provider: 'github' | 'gitlab', account: AccountRow) {
  if (busyGitAccountId.value) return
  busyGitAccountId.value = account.id
  try {
    if (provider === 'github') await projectStore.setProjectAccount(props.project.id, account.id)
    else await projectStore.setProjectGitlabAccount(props.project.id, account.id)
    toast.success(t('projectDetail.toastDefaultAccount', { name: account.label }))
  } catch (e) {
    toast.error(String(e))
  } finally {
    busyGitAccountId.value = null
  }
}

// --- AWS / Azure accounts (several per project, toggled like Git accounts) ---
const awsAccountRows = computed<AccountRow[]>(() => awsStore.accounts.map(a => ({
  id: a.id,
  label: a.label,
  username: null,
  detail: [a.auth_method.toUpperCase(), a.region, a.account_id, a.profile_name].filter(Boolean).join(' · '),
})))
const azureAccountRows = computed<AccountRow[]>(() => azureStore.accounts.map(a => ({
  id: a.id,
  label: a.label,
  username: null,
  detail: [a.subscription_name, a.subscription_id].filter(Boolean).join(' · ') || null,
  warning: a.logged_in ? null : t('projectDetail.azureNotSignedIn'),
})))
const busyCloudAccountId = ref<string | null>(null)

async function handleToggleCloudAccount(provider: 'aws' | 'azure', account: AccountRow, enabled: boolean) {
  if (busyCloudAccountId.value) return
  busyCloudAccountId.value = account.id
  try {
    await projectStore.setProjectCloudAccountEnabled(props.project.id, provider, account.id, enabled)
    toast.success(t(enabled ? 'projectDetail.toastAccountEnabled' : 'projectDetail.toastAccountDisabled', { name: account.label }))
  } catch (e) {
    toast.error(String(e))
  } finally {
    busyCloudAccountId.value = null
  }
}

async function handleMakeDefaultCloudAccount(provider: 'aws' | 'azure', account: AccountRow) {
  if (busyCloudAccountId.value) return
  busyCloudAccountId.value = account.id
  try {
    if (provider === 'aws') await projectStore.setProjectAwsAccount(props.project.id, account.id)
    else await projectStore.setProjectAzureAccount(props.project.id, account.id)
    toast.success(t('projectDetail.toastDefaultAccount', { name: account.label }))
  } catch (e) {
    toast.error(String(e))
  } finally {
    busyCloudAccountId.value = null
  }
}
</script>

<template>
  <div class="max-w-lg space-y-4">
    <ProjectAccountsList
      v-if="section === 'github'"
      provider="github"
      :accounts="githubAccountRows"
      :enabled-ids="project.github_account_ids"
      :default-id="project.github_account_id ?? null"
      :busy="busyGitAccountId"
      @toggle="(a, on) => handleToggleGitAccount('github', a, on)"
      @make-default="(a) => handleMakeDefaultGitAccount('github', a)"
    />
    <ProjectAccountsList
      v-else-if="section === 'gitlab'"
      provider="gitlab"
      :accounts="gitlabAccountRows"
      :enabled-ids="project.gitlab_account_ids"
      :default-id="project.gitlab_account_id ?? null"
      :busy="busyGitAccountId"
      @toggle="(a, on) => handleToggleGitAccount('gitlab', a, on)"
      @make-default="(a) => handleMakeDefaultGitAccount('gitlab', a)"
    />
    <ProjectAccountsList
      v-else-if="section === 'aws'"
      provider="aws"
      :accounts="awsAccountRows"
      :enabled-ids="project.aws_account_ids"
      :default-id="project.aws_account_id ?? null"
      :busy="busyCloudAccountId"
      @toggle="(a, on) => handleToggleCloudAccount('aws', a, on)"
      @make-default="(a) => handleMakeDefaultCloudAccount('aws', a)"
    />
    <ProjectAccountsList
      v-else
      provider="azure"
      :accounts="azureAccountRows"
      :enabled-ids="project.azure_account_ids"
      :default-id="project.azure_account_id ?? null"
      :busy="busyCloudAccountId"
      @toggle="(a, on) => handleToggleCloudAccount('azure', a, on)"
      @make-default="(a) => handleMakeDefaultCloudAccount('azure', a)"
    />
  </div>
</template>
