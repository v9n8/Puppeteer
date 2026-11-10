<script setup lang="ts">
// Settings → "AWS Accounts" (profile/SSO only): add from the local ~/.aws
// profiles in one click, validate, SSO login, edit and delete. SettingsView
// fetches the account store before showing the sections (the nav badge counts it).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Cloud, LogIn, Pencil, Plus, RefreshCw, ShieldAlert, Trash2 } from 'lucide-vue-next'
import { Banner, Button, Card, Input } from '@/components/ui'
import AwsProfilePicker from '@/components/AwsProfilePicker.vue'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import {
  useAwsAccountsStore, isSsoLoginRequired,
  type AwsAccountPayload, type AwsProfileInfo, type AwsValidation,
} from '@/stores/awsAccounts'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()

const awsStore = useAwsAccountsStore()
const awsEditLabel = ref<Record<string, string>>({})
const awsEditRegion = ref<Record<string, string>>({})
const awsEditProfileName = ref<Record<string, string>>({})
const awsEditTags = ref<Record<string, string>>({})
const awsEditing = ref<string | null>(null)
const awsValidations = ref<Record<string, AwsValidation>>({})
const awsAccountError = ref<Record<string, string>>({})
const awsBusyAccount = ref<string | null>(null)
// Accounts whose last validation failed because their SSO token needs refreshing.
const awsSsoLoginNeeded = ref<Record<string, boolean>>({})

// Local ~/.aws profiles available to add, one click each.
const awsProfiles = ref<AwsProfileInfo[]>([])
const awsProfilesLoading = ref(false)
const awsAddError = ref<string | null>(null)
const awsAddingProfile = ref<string | null>(null) // profile name currently being added
const awsSsoLoginProfile = ref<string | null>(null) // profile whose token needs a refresh

async function loadAwsProfiles() {
  awsProfilesLoading.value = true
  try {
    awsProfiles.value = await awsStore.listProfiles()
  } catch (e) {
    awsAddError.value = String(e)
  } finally {
    awsProfilesLoading.value = false
  }
}

// Profiles not already linked (compared by profile name).
const addableAwsProfiles = computed(() => {
  const used = new Set(awsStore.accounts.map((a) => a.profile_name).filter(Boolean))
  return awsProfiles.value.filter((p) => !used.has(p.name))
})

function awsProfileCaption(p: AwsProfileInfo): string {
  const parts: string[] = []
  if (p.isSso) {
    parts.push(p.ssoRoleName ?? 'SSO')
    if (p.ssoAccountId) parts.push(p.ssoAccountId)
  } else {
    parts.push(t('settings.aws.profileNonSso'))
  }
  if (p.region) parts.push(p.region)
  return parts.join(' · ')
}

async function handleQuickAddProfile(p: AwsProfileInfo) {
  awsAddingProfile.value = p.name
  awsAddError.value = null
  awsSsoLoginProfile.value = null
  try {
    await awsStore.create({ label: p.name, region: p.region ?? undefined, profileName: p.name })
    toast.success(t('settings.github.accountAdded'))
  } catch (e) {
    if (isSsoLoginRequired(e)) awsSsoLoginProfile.value = p.name
    else awsAddError.value = String(e)
  } finally {
    awsAddingProfile.value = null
  }
}

// The profile's SSO token expired → run `aws sso login`, then retry the add.
async function handleQuickAddSsoLogin(p: AwsProfileInfo) {
  awsAddingProfile.value = p.name
  awsAddError.value = null
  try {
    await awsStore.ssoLoginProfile(p.name)
    awsSsoLoginProfile.value = null
    await awsStore.create({ label: p.name, region: p.region ?? undefined, profileName: p.name })
    toast.success(t('settings.github.accountAdded'))
  } catch (e) {
    if (isSsoLoginRequired(e)) awsSsoLoginProfile.value = p.name
    else awsAddError.value = String(e)
  } finally {
    awsAddingProfile.value = null
  }
}

function awsPayloadForEdit(id: string): AwsAccountPayload {
  return {
    label: awsEditLabel.value[id]?.trim() || '',
    region: awsEditRegion.value[id]?.trim() || 'ap-northeast-1',
    profileName: awsEditProfileName.value[id]?.trim(),
    tags: awsEditTags.value[id]?.trim(),
  }
}
function onAwsEditProfilePicked(id: string, p: AwsProfileInfo) {
  if (p.region) awsEditRegion.value[id] = p.region
  if (!awsEditLabel.value[id]?.trim()) awsEditLabel.value[id] = p.name
}

function startAwsEdit(acc: { id: string; label: string; region: string; profile_name: string | null; tags: string | null }) {
  awsEditing.value = acc.id
  awsEditLabel.value[acc.id] = acc.label
  awsEditRegion.value[acc.id] = acc.region
  awsEditProfileName.value[acc.id] = acc.profile_name ?? ''
  awsEditTags.value[acc.id] = acc.tags ?? ''
}

async function handleSaveAwsEdit(id: string) {
  awsBusyAccount.value = id
  awsAccountError.value[id] = ''
  try {
    await awsStore.update(id, awsPayloadForEdit(id))
    awsEditing.value = null
    toast.success(t('settings.github.accountUpdated'))
  } catch (e) {
    awsAccountError.value[id] = String(e)
  } finally {
    awsBusyAccount.value = null
  }
}

async function handleValidateAws(id: string) {
  awsBusyAccount.value = id
  awsAccountError.value[id] = ''
  awsSsoLoginNeeded.value[id] = false
  delete awsValidations.value[id]
  try {
    awsValidations.value[id] = await awsStore.validate(id)
  } catch (e) {
    // "Needs login" is a state (amber notice), not a hard error (red).
    if (isSsoLoginRequired(e)) {
      awsSsoLoginNeeded.value[id] = true
    } else {
      awsAccountError.value[id] = String(e)
    }
  } finally {
    awsBusyAccount.value = null
  }
}

async function handleAwsSsoLogin(id: string) {
  awsBusyAccount.value = id
  awsAccountError.value[id] = ''
  try {
    await awsStore.ssoLogin(id)
    awsSsoLoginNeeded.value[id] = false
    // Re-validate straight away so a successful login turns the row green.
    await handleValidateAws(id)
  } catch (e) {
    awsAccountError.value[id] = String(e)
  } finally {
    if (awsBusyAccount.value === id) awsBusyAccount.value = null
  }
}

async function handleDeleteAwsAccount(id: string) {
  if (!(await confirm({
    title: t('settings.aws.deleteTitle'),
    message: t('settings.aws.deleteMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  try {
    await awsStore.remove(id)
    toast.success(t('settings.github.accountDeleted'))
  } catch (e) {
    toast.error(String(e))
  }
}

onMounted(() => {
  loadAwsProfiles()
})
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Cloud class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.aws.title') }}</span>
    </template>
      <p class="text-2xs text-muted-foreground leading-relaxed">
        {{ t('settings.aws.intro') }}
      </p>

      <Banner :icon="ShieldAlert" class="leading-relaxed">
        <span class="text-muted-foreground">
          {{ t('settings.aws.isolationWarning') }}
        </span>
      </Banner>

      <!-- Account list -->
      <div v-if="awsStore.accounts.length" class="space-y-2">
        <div
          v-for="acc in awsStore.accounts"
          :key="acc.id"
          class="border border-border rounded-md p-3 space-y-2"
        >
          <template v-if="awsEditing !== acc.id">
            <div class="flex items-center justify-between gap-2">
              <div class="min-w-0">
                <div class="text-sm font-medium truncate">{{ acc.label }}</div>
                <div class="text-2xs text-muted-foreground truncate">
                  <span v-if="acc.profile_name" class="font-mono">{{ acc.profile_name }}</span>
                  <span> · {{ acc.region }}</span>
                  <span v-if="acc.account_id"> · {{ acc.account_id }}</span>
                </div>
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <Button
                  variant="outline"
                  size="xs"
                  :disabled="awsBusyAccount === acc.id"
                  @click="handleValidateAws(acc.id)"
                >
                  {{ awsBusyAccount === acc.id ? '…' : t('settings.aws.validate') }}
                </Button>
                <Button
                  v-if="awsSsoLoginNeeded[acc.id]"
                  variant="outline"
                  size="xs"
                  :disabled="awsBusyAccount === acc.id"
                  @click="handleAwsSsoLogin(acc.id)"
                >
                  <LogIn class="h-3 w-3" :stroke-width="1.75" />
                  {{ t('settings.aws.ssoLogin') }}
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  :title="t('common.edit')"
                  :aria-label="t('common.edit')"
                  @click="startAwsEdit(acc)"
                >
                  <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
                <Button
                  variant="destructive-ghost"
                  size="icon-sm"
                  :title="t('common.delete')"
                  :aria-label="t('common.delete')"
                  @click="handleDeleteAwsAccount(acc.id)"
                >
                  <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
              </div>
            </div>
            <Banner
              v-if="awsValidations[acc.id]"
              tone="success"
            >
              <div class="font-medium text-success">{{ t('settings.aws.valid') }}</div>
              <p v-if="awsValidations[acc.id].account_id" class="text-muted-foreground mt-1 truncate">{{ awsValidations[acc.id].account_id }}</p>
              <p class="text-muted-foreground mt-1 truncate">{{ awsValidations[acc.id].arn }}</p>
            </Banner>
            <Banner
              v-if="awsSsoLoginNeeded[acc.id]"
              tone="warning"
            >
              <span>{{ awsBusyAccount === acc.id ? t('settings.aws.ssoLoggingIn') : t('settings.aws.ssoExpired') }}</span>
            </Banner>
          </template>

          <template v-else>
            <AwsProfilePicker
              v-model="awsEditProfileName[acc.id]"
              @picked="onAwsEditProfilePicked(acc.id, $event)"
            />
            <Input v-model="awsEditLabel[acc.id]" size="sm" :placeholder="t('settings.aws.labelPlaceholder')" />
            <Input v-model="awsEditRegion[acc.id]" size="sm" :placeholder="t('settings.aws.regionPlaceholder')" class="font-mono" />
            <Input v-model="awsEditTags[acc.id]" size="sm" :placeholder="t('settings.aws.tagsPlaceholder')" />
            <div class="flex items-center gap-2">
              <Button
                :disabled="!awsEditLabel[acc.id]?.trim() || awsBusyAccount === acc.id"
                @click="handleSaveAwsEdit(acc.id)"
              >
                {{ awsBusyAccount === acc.id ? t('settings.aws.saving') : t('common.save') }}
              </Button>
              <Button variant="outline" @click="awsEditing = null">{{ t('common.cancel') }}</Button>
            </div>
          </template>

          <p v-if="awsAccountError[acc.id]" class="text-2xs text-destructive">{{ awsAccountError[acc.id] }}</p>
        </div>
      </div>

      <!-- Add account: one-click from the local ~/.aws profiles -->
      <div class="border-t border-border/60 pt-3 space-y-2">
        <div class="flex items-center justify-between">
          <div class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.aws.addFromProfile') }}</div>
          <Button variant="ghost" size="icon-sm" :title="t('common.refresh')" :aria-label="t('common.refresh')" :disabled="awsProfilesLoading" @click="loadAwsProfiles">
            <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': awsProfilesLoading }" :stroke-width="1.75" />
          </Button>
        </div>
        <p v-if="awsProfilesLoading" class="text-2xs text-muted-foreground">{{ t('common.loading') }}</p>
        <p v-else-if="!addableAwsProfiles.length" class="text-2xs text-muted-foreground">{{ t('settings.aws.profileAllAdded') }}</p>
        <div
          v-for="p in addableAwsProfiles"
          :key="p.name"
          class="flex items-center justify-between gap-2 border border-border rounded-md p-2"
        >
          <div class="min-w-0">
            <div class="text-sm font-medium truncate font-mono">{{ p.name }}</div>
            <div class="text-2xs text-muted-foreground truncate">{{ awsProfileCaption(p) }}</div>
          </div>
          <Button
            v-if="awsSsoLoginProfile === p.name"
            size="xs"
            variant="outline"
            :disabled="awsAddingProfile === p.name"
            @click="handleQuickAddSsoLogin(p)"
          >
            <LogIn class="h-3 w-3" :stroke-width="1.75" />
            {{ awsAddingProfile === p.name ? t('settings.aws.ssoLoggingIn') : t('settings.aws.ssoLogin') }}
          </Button>
          <Button
            v-else
            size="xs"
            :disabled="awsAddingProfile === p.name"
            @click="handleQuickAddProfile(p)"
          >
            <Plus class="h-3 w-3" :stroke-width="2" />
            {{ awsAddingProfile === p.name ? t('settings.aws.adding') : t('common.add') }}
          </Button>
        </div>
        <p v-if="awsAddError" class="text-2xs text-destructive">{{ awsAddError }}</p>
      </div>
  </Card>
</template>
