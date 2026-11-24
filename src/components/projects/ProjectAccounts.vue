<script setup lang="ts">
// Accounts section of the Connections tab: every GitHub / GitLab / AWS / Azure
// account the project may use in one list, filtered by type like Settings >
// Connections (V3-44). Each account has an on/off switch and "Make default".
// Sessions may use any enabled account; commands without PUPPETEER_GIT_ACCOUNT /
// PUPPETEER_AWS_ACCOUNT / PUPPETEER_AZURE_ACCOUNT use the default.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink } from 'vue-router'
import { Badge, Banner, Card, InfoTip, SegmentedControl, ToggleSwitch, type SegmentedOption } from '@/components/ui'
import { useConnectionSources } from '@/components/settings/useConnectionSources'
import { useToast } from '@/composables/useToast'
import { CONNECTION_TYPES, connectionType, type AccountLike } from '@/lib/connectionTypes'
import { parentLock, switchView } from '@/lib/configLock'
import { PROJECT_ACCOUNT_TYPES, type ProjectAccountType } from '@/lib/projectSections'
import { useProjectsStore, type Project } from '@/stores/projects'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useAzureAccountsStore } from '@/stores/azureAccounts'

const props = defineProps<{ project: Project; type: ProjectAccountType | null }>()

const { t } = useI18n()
const { toast } = useToast()
const projectStore = useProjectsStore()
const { sources, fetchers, types } = useConnectionSources()

// The stores keep a failed load in `error` (fetch never throws); it is shown below instead of "No accounts yet".
const accountStores = {
  github: useGithubAccountsStore(),
  gitlab: useGitlabAccountsStore(),
  aws: useAwsAccountsStore(),
  azure: useAzureAccountsStore(),
}
const loadError = computed(() => PROJECT_ACCOUNT_TYPES.map((id) => accountStores[id].error).find(Boolean) ?? null)

for (const id of PROJECT_ACCOUNT_TYPES) if (!sources.value[id]?.length) fetchers[id]()

// Per type: the strings of its hint line.
const CONFIG = {
  github: { hint: 'gitAccountsHint', empty: 'noGithubAccounts', envVar: 'PUPPETEER_GIT_ACCOUNT' },
  gitlab: { hint: 'gitAccountsHint', empty: 'noGitlabAccounts', envVar: 'PUPPETEER_GIT_ACCOUNT' },
  aws: { hint: 'awsAccountHint', empty: 'noAwsAccounts', envVar: 'PUPPETEER_AWS_ACCOUNT' },
  azure: { hint: 'azureAccountHint', empty: 'noAzureAccounts', envVar: 'PUPPETEER_AZURE_ACCOUNT' },
} as const

const enabledIds = (type: ProjectAccountType) => new Set(props.project[connectionType(type).projectField!])
const defaultId = (type: ProjectAccountType): string | null => props.project[`${type}_account_id`] ?? null

// Every type a project binds. One whose feature is off (AWS / Azure, infra_access.*) is shown
// locked rather than hidden: its switches show off and the project's choice is kept.
const shownTypes = computed(() => CONNECTION_TYPES.filter((d) => (PROJECT_ACCOUNT_TYPES as readonly string[]).includes(d.id)))
const isLocked = (type: ProjectAccountType) => parentLock(types.value.some((d) => d.id === type)) !== null
const accountView = (type: ProjectAccountType, account: AccountLike) =>
  switchView(enabledIds(type).has(account.id), parentLock(!isLocked(type)))
const accountsOf = (type: ProjectAccountType): readonly AccountLike[] => sources.value[type] ?? []
const total = computed(() => shownTypes.value.reduce((n, d) => n + accountsOf(d.id as ProjectAccountType).length, 0))

type Filter = ProjectAccountType | 'all'
const filter = ref<Filter>(props.type ?? 'all')
watch(() => props.type, (v) => { filter.value = v ?? 'all' })
// A filter left on a type that got switched off falls back to all.
const activeFilter = computed<Filter>(() => (types.value.some((d) => d.id === filter.value) ? filter.value : 'all'))

const filterOptions = computed<SegmentedOption<Filter>[]>(() => [
  { value: 'all', label: `${t('settings.connections.all')} ${total.value}` },
  ...shownTypes.value.map((d) => ({
    value: d.id as ProjectAccountType,
    label: `${t(d.labelKey)} ${accountsOf(d.id as ProjectAccountType).length}`,
    icon: d.icon,
    disabled: isLocked(d.id as ProjectAccountType),
    title: isLocked(d.id as ProjectAccountType) ? t('projectDetail.lockedFeature') : undefined,
  })),
])
const groups = computed(() => shownTypes.value
  .map((d) => ({ def: d, type: d.id as ProjectAccountType, accounts: accountsOf(d.id as ProjectAccountType) }))
  .filter((g) => (activeFilter.value === 'all' ? g.accounts.length > 0 : g.type === activeFilter.value)))

const manageTo = computed(() => ({
  path: '/settings/connections',
  query: activeFilter.value === 'all' ? {} : { type: activeFilter.value },
}))

const busyId = ref<string | null>(null)

async function run(account: AccountLike, action: () => Promise<void>, successKey: string) {
  if (busyId.value) return
  busyId.value = account.id
  try {
    await action()
    toast.success(t(successKey, { name: account.label }))
  } catch (e) {
    toast.error(String(e))
  } finally {
    busyId.value = null
  }
}

function toggle(type: ProjectAccountType, account: AccountLike, enabled: boolean) {
  const id = props.project.id
  const action = type === 'github' || type === 'gitlab'
    ? () => projectStore.setProjectGitAccountEnabled(id, type, account.id, enabled)
    : () => projectStore.setProjectCloudAccountEnabled(id, type, account.id, enabled)
  return run(account, action, enabled ? 'projectDetail.toastAccountEnabled' : 'projectDetail.toastAccountDisabled')
}

function makeDefault(type: ProjectAccountType, account: AccountLike) {
  const id = props.project.id
  const setters: Record<ProjectAccountType, () => Promise<void>> = {
    github: () => projectStore.setProjectAccount(id, account.id),
    gitlab: () => projectStore.setProjectGitlabAccount(id, account.id),
    aws: () => projectStore.setProjectAwsAccount(id, account.id),
    azure: () => projectStore.setProjectAzureAccount(id, account.id),
  }
  return run(account, setters[type], 'projectDetail.toastDefaultAccount')
}
</script>

<template>
  <div class="max-w-lg space-y-3">
    <SegmentedControl
      :model-value="activeFilter"
      :options="filterOptions"
      :stretch="false"
      class="flex-wrap [&_button]:min-h-7"
      @update:model-value="filter = $event"
    />

    <Card>
      <template #header>
        <span class="text-xs font-semibold">{{ t('projectDetail.sections.accounts') }}</span>
        <InfoTip
          v-if="activeFilter !== 'all'"
          :text="`${t(`projectDetail.${CONFIG[activeFilter].hint}`)} ${CONFIG[activeFilter].envVar}=<label>`"
        />
        <RouterLink :to="manageTo" class="ml-auto inline-flex min-h-7 items-center px-1 text-2xs text-primary hover:underline">{{ t('projectDetail.manage') }}</RouterLink>
      </template>

      <Banner v-if="loadError" tone="destructive" live class="m-3">
        {{ t('projectDetail.accountsLoadError', { error: loadError }) }}
      </Banner>
      <div v-else-if="total === 0" class="px-4 py-5 text-center text-xs text-muted-foreground">
        {{ t('projectDetail.noAccounts') }}
        <RouterLink :to="manageTo" class="text-primary hover:underline">{{ t('projectDetail.addInSettings') }}</RouterLink>
        {{ t('projectDetail.toLinkHere') }}
      </div>

      <div v-else class="mt-2 border-t border-border/50">
        <section v-for="g in groups" :key="g.type">
          <h3
            v-if="activeFilter === 'all'"
            class="flex items-center gap-1.5 bg-muted/30 px-4 py-1.5 text-2xs font-medium uppercase tracking-wide text-muted-foreground"
            :title="isLocked(g.type) ? t('projectDetail.lockedFeature') : undefined"
          >
            <component :is="g.def.icon" class="h-3.5 w-3.5" :stroke-width="1.5" />
            {{ t(g.def.labelKey) }}
            <span class="tabular-nums">{{ g.accounts.length }}</span>
          </h3>
          <p v-if="g.accounts.length === 0" class="px-4 py-5 text-center text-xs text-muted-foreground">
            {{ t(`projectDetail.${CONFIG[g.type].empty}`) }}
          </p>
          <div v-else class="divide-y divide-border/50">
            <div v-for="account in g.accounts" :key="account.id" class="flex items-center gap-3 px-4 py-3">
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <p class="truncate text-xs font-medium">{{ account.label }}</p>
                  <Badge
                    v-if="account.id === defaultId(g.type)"
                    tone="primary"
                    size="xs"
                    class="shrink-0 uppercase tracking-wide"
                  >{{ t('projectDetail.defaultAccount') }}</Badge>
                  <button
                    v-else-if="accountView(g.type, account).checked"
                    type="button"
                    class="-my-1 inline-flex min-h-7 items-center px-1 text-2xs text-primary hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50"
                    :disabled="busyId !== null"
                    @click="makeDefault(g.type, account)"
                  >{{ t('projectDetail.makeDefault') }}</button>
                </div>
                <p class="mt-0.5 truncate font-mono text-2xs text-muted-foreground">
                  {{ g.def.detail(account) }}
                  <span v-if="g.type === 'azure' && account.logged_in === false" class="font-sans text-warning">
                    · {{ t('projectDetail.azureNotSignedIn') }}
                  </span>
                </p>
              </div>
              <ToggleSwitch
                :checked="accountView(g.type, account).checked"
                :disabled="busyId !== null || accountView(g.type, account).disabled"
                :loading="busyId === account.id"
                :label="isLocked(g.type) ? t('projectDetail.lockedFeature') : enabledIds(g.type).has(account.id) ? t('projectDetail.disableForProject') : t('projectDetail.enableForProject')"
                @toggle="toggle(g.type, account, !enabledIds(g.type).has(account.id))"
              />
            </div>
          </div>
        </section>
      </div>
    </Card>
  </div>
</template>
