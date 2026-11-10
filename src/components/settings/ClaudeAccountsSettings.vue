<script setup lang="ts">
// Settings → "Claude Accounts" (multi-account via CLAUDE_CONFIG_DIR): browser
// login, validate, default account and the auto-balancer that moves the default.
// SettingsView fetches the account store before showing the sections; the
// auto-balance fields are keys of the shared settings form.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useI18n } from 'vue-i18n'
import {
  ArrowRightLeft, LogIn, Pencil, Plus, ShieldAlert, Star, Trash2, UserCircle,
} from 'lucide-vue-next'
import { Banner, Button, Card, Input, ToggleSwitch } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useClaudeAccountsStore, type ClaudeValidation } from '@/stores/claudeAccounts'
import { useBudgetStore } from '@/stores/budget'
import { useSettingsForm } from './useSettingsForm'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const budget = useBudgetStore()
const { settings, loading } = useSettingsForm()

let unlistenDefaultChanged: UnlistenFn | null = null

const claudeStore = useClaudeAccountsStore()
const claudeNewLabel = ref('')
const claudeAdding = ref(false)
const claudeAddError = ref<string | null>(null)
const claudeEditing = ref<string | null>(null)
const claudeEditLabel = ref<Record<string, string>>({})
const claudeBusyAccount = ref<string | null>(null)
const claudeAccountError = ref<Record<string, string>>({})
// Last validation result per account, shown inline (mirrors the AWS section)
// instead of via a toast.
const claudeValidations = ref<Record<string, ClaudeValidation>>({})

function claudeStatusTone(status: string): string {
  switch (status) {
    case 'ready': return 'text-success'
    case 'needs_login': return 'text-warning'
    case 'error': return 'text-destructive'
    default: return 'text-muted-foreground'
  }
}

// Label of the account the balancer currently suggests as default.
const balanceRecommendedLabel = computed(() => {
  const id = claudeStore.balance?.recommended_id
  return claudeStore.accounts.find((a) => a.id === id)?.label ?? ''
})
// Whether the balancer is flagging a worthwhile swap off the current default.
const showBalanceSuggestion = computed(() => {
  const b = claudeStore.balance
  return !!b && b.should_swap && !!b.recommended_id
    && b.recommended_id !== b.current_default_id
})

// Re-read the balance verdict shortly after the balance settings change (the
// backend reads them from the DB, so wait out the debounced persist).
watch(
  () => [
    settings.value.claude_auto_balance,
    settings.value.claude_auto_balance_swap_threshold_pct,
    settings.value.claude_auto_balance_min_swap_minutes,
  ],
  () => {
    if (loading.value) return
    setTimeout(() => claudeStore.fetchBalance(), 700)
  },
)

async function handleAddClaudeAccount() {
  const label = claudeNewLabel.value.trim()
  if (!label) return
  claudeAdding.value = true
  claudeAddError.value = null
  try {
    const acc = await claudeStore.create(label)
    claudeNewLabel.value = ''
    toast.success(t('settings.claude.accountAdded'))
    // Immediately start the browser login for the fresh profile.
    void startClaudeLogin(acc.id)
  } catch (e) {
    claudeAddError.value = String(e)
  } finally {
    claudeAdding.value = false
  }
}

function startClaudeEdit(acc: { id: string; label: string }) {
  claudeEditing.value = acc.id
  claudeEditLabel.value[acc.id] = acc.label
}

async function handleSaveClaudeEdit(id: string) {
  claudeBusyAccount.value = id
  claudeAccountError.value[id] = ''
  try {
    await claudeStore.rename(id, claudeEditLabel.value[id]?.trim() || '')
    claudeEditing.value = null
    toast.success(t('settings.github.accountUpdated'))
  } catch (e) {
    claudeAccountError.value[id] = String(e)
  } finally {
    claudeBusyAccount.value = null
  }
}

// `claude auth login` runs in the background and opens the browser; the
// command resolves when the CLI exits. If the browser can't reach the CLI's
// localhost callback, the manual-login page shows a code the user pastes here.
const claudeLoginWaiting = ref<Record<string, boolean>>({})
const claudeLoginUrl = ref<Record<string, string>>({})
const claudeLoginCode = ref<Record<string, string>>({})
let unlistenClaudeLoginUrl: UnlistenFn | null = null

async function startClaudeLogin(id: string) {
  if (claudeLoginWaiting.value[id]) return
  claudeAccountError.value[id] = ''
  delete claudeValidations.value[id]
  claudeLoginWaiting.value[id] = true
  toast.info(t('settings.claude.loginStarted'))
  try {
    const result = await claudeStore.login(id)
    claudeValidations.value[id] = result
    if (result.logged_in) toast.success(t('settings.claude.loginDetected', { email: result.email ?? '' }))
  } catch (e) {
    const msg = String(e)
    if (!msg.includes('cancelled')) claudeAccountError.value[id] = msg
  } finally {
    delete claudeLoginWaiting.value[id]
    delete claudeLoginUrl.value[id]
    delete claudeLoginCode.value[id]
  }
}

function handleClaudeLogin(id: string) {
  void startClaudeLogin(id)
}

async function handleSubmitClaudeCode(id: string) {
  const code = claudeLoginCode.value[id]?.trim()
  if (!code) return
  try {
    await claudeStore.submitLoginCode(id, code)
    claudeLoginCode.value[id] = ''
  } catch (e) {
    claudeAccountError.value[id] = String(e)
  }
}

async function handleCancelClaudeLogin(id: string) {
  await claudeStore.cancelLogin(id).catch(() => {})
}

function openClaudeLoginUrl(id: string) {
  const url = claudeLoginUrl.value[id]
  if (url) void openUrl(url).catch(() => {})
}

async function handleValidateClaude(id: string) {
  claudeBusyAccount.value = id
  claudeAccountError.value[id] = ''
  delete claudeValidations.value[id]
  try {
    claudeValidations.value[id] = await claudeStore.validate(id)
  } catch (e) {
    claudeAccountError.value[id] = String(e)
  } finally {
    claudeBusyAccount.value = null
  }
}

async function handleSetDefaultClaude(id: string) {
  try {
    await claudeStore.setDefault(id)
    // The desktop badge follows the default account — refresh it now.
    budget.refresh()
  } catch (e) {
    toast.error(String(e))
  }
}

async function handleDeleteClaudeAccount(id: string) {
  if (!(await confirm({
    title: t('settings.claude.deleteTitle'),
    message: t('settings.claude.deleteMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  try {
    await claudeStore.remove(id)
    toast.success(t('settings.github.accountDeleted'))
  } catch (e) {
    toast.error(String(e))
  }
}

onMounted(async () => {
  // Auto-balancer swapped the default account → refresh the list + star + badge.
  unlistenDefaultChanged = await listen('claude_default_changed', () => claudeStore.fetch())
  unlistenClaudeLoginUrl = await listen<{ accountId: string; url: string }>('claude_login_url', (e) => {
    if (claudeLoginWaiting.value[e.payload.accountId]) claudeLoginUrl.value[e.payload.accountId] = e.payload.url
  })
})

onUnmounted(() => {
  if (unlistenDefaultChanged) unlistenDefaultChanged()
  if (unlistenClaudeLoginUrl) unlistenClaudeLoginUrl()
  // Don't leave a background `claude auth login` behind a screen that's gone.
  for (const id of Object.keys(claudeLoginWaiting.value)) void claudeStore.cancelLogin(id).catch(() => {})
})
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <UserCircle class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.claude.title') }}</span>
    </template>
      <p class="text-2xs text-muted-foreground leading-relaxed">
        {{ t('settings.claude.intro') }}
      </p>

      <Banner :icon="ShieldAlert" class="leading-relaxed">
        <span class="text-muted-foreground">
          {{ t('settings.claude.securityNote') }}
        </span>
      </Banner>

      <!-- Account list -->
      <div v-if="claudeStore.accounts.length" class="space-y-2">
        <div
          v-for="acc in claudeStore.accounts"
          :key="acc.id"
          class="border border-border rounded-md p-3 space-y-2"
        >
          <template v-if="claudeEditing !== acc.id">
            <div class="flex items-center justify-between gap-2">
              <div class="min-w-0">
                <div class="flex items-center gap-1.5">
                  <span class="text-sm font-medium truncate">{{ acc.label }}</span>
                  <span
                    v-if="acc.is_default"
                    class="inline-flex items-center gap-0.5 text-2xs text-primary font-medium"
                  >
                    <Star class="h-2.5 w-2.5" :stroke-width="2" fill="currentColor" />
                    {{ t('settings.claude.default') }}
                  </span>
                </div>
                <div class="text-2xs truncate" :class="claudeStatusTone(acc.status)">
                  {{ t(`settings.claude.status.${acc.status}`) }}
                  <span v-if="claudeLoginWaiting[acc.id]" class="text-muted-foreground"> · {{ t('settings.claude.waitingLogin') }}</span>
                  <span v-if="acc.email" class="text-muted-foreground"> · {{ acc.email }}</span>
                  <span v-else-if="acc.auth_method" class="text-muted-foreground"> · {{ acc.auth_method }}</span>
                  <span
                    v-if="claudeStore.budgets[acc.id]?.source === 'plan'"
                    class="text-muted-foreground"
                    :class="{
                      'text-warning': claudeStore.budgets[acc.id]?.is_warning,
                      'text-destructive': claudeStore.budgets[acc.id]?.is_over,
                    }"
                  >
                    · {{ t('settings.claude.usagePercent', {
                      percent: claudeStore.budgets[acc.id].percent,
                      period: t(`settings.claude.period.${claudeStore.budgets[acc.id].period}`),
                    }) }}
                  </span>
                </div>
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <Button
                  variant="outline"
                  size="xs"
                  :disabled="claudeBusyAccount === acc.id"
                  @click="handleValidateClaude(acc.id)"
                >
                  {{ claudeBusyAccount === acc.id ? '…' : t('settings.claude.validate') }}
                </Button>
                <Button
                  v-if="claudeLoginWaiting[acc.id]"
                  variant="outline"
                  size="xs"
                  @click="handleCancelClaudeLogin(acc.id)"
                >
                  {{ t('common.cancel') }}
                </Button>
                <Button
                  v-else-if="acc.status !== 'ready'"
                  variant="outline"
                  size="xs"
                  @click="handleClaudeLogin(acc.id)"
                >
                  <LogIn class="h-3 w-3" :stroke-width="1.75" />
                  {{ t('settings.claude.login') }}
                </Button>
                <Button
                  v-if="!acc.is_default"
                  variant="ghost"
                  size="icon-sm"
                  :title="t('settings.claude.setDefault')"
                  :aria-label="t('settings.claude.setDefault')"
                  @click="handleSetDefaultClaude(acc.id)"
                >
                  <Star class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  :title="t('common.edit')"
                  :aria-label="t('common.edit')"
                  @click="startClaudeEdit(acc)"
                >
                  <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
                <Button
                  variant="destructive-ghost"
                  size="icon-sm"
                  :title="t('common.delete')"
                  :aria-label="t('common.delete')"
                  @click="handleDeleteClaudeAccount(acc.id)"
                >
                  <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
              </div>
            </div>
            <div
              v-if="claudeLoginWaiting[acc.id]"
              class="p-2 bg-muted/40 border border-border rounded-md text-2xs space-y-1.5"
            >
              <p class="text-muted-foreground">{{ t('settings.claude.loginWaitingHint') }}</p>
              <template v-if="claudeLoginUrl[acc.id]">
                <button
                  type="button"
                  class="text-primary hover:underline"
                  @click="openClaudeLoginUrl(acc.id)"
                >
                  {{ t('settings.claude.loginManualLink') }}
                </button>
                <div class="flex items-center gap-1.5">
                  <Input
                    v-model="claudeLoginCode[acc.id]"
                    size="sm"
                    class="flex-1"
                    :placeholder="t('settings.claude.loginCodePlaceholder')"
                    @keyup.enter="handleSubmitClaudeCode(acc.id)"
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    :disabled="!claudeLoginCode[acc.id]?.trim()"
                    @click="handleSubmitClaudeCode(acc.id)"
                  >
                    {{ t('settings.claude.loginSubmitCode') }}
                  </Button>
                </div>
              </template>
            </div>
            <Banner
              v-else-if="claudeValidations[acc.id]?.logged_in"
              tone="success"
            >
              <div class="font-medium text-success">{{ t('settings.claude.validReady') }}</div>
              <p v-if="claudeValidations[acc.id].email" class="text-muted-foreground mt-1 truncate">
                {{ claudeValidations[acc.id].email }}
              </p>
            </Banner>
            <Banner
              v-else-if="claudeValidations[acc.id]"
              tone="warning"
            >
              <span>{{ t('settings.claude.validNeedsLogin') }}</span>
            </Banner>
          </template>

          <template v-else>
            <Input
              v-model="claudeEditLabel[acc.id]"
              size="sm"
              :placeholder="t('settings.claude.labelPlaceholder')"
              @keyup.enter="handleSaveClaudeEdit(acc.id)"
            />
            <div class="flex items-center gap-2">
              <Button
                :disabled="!claudeEditLabel[acc.id]?.trim() || claudeBusyAccount === acc.id"
                @click="handleSaveClaudeEdit(acc.id)"
              >
                {{ claudeBusyAccount === acc.id ? t('settings.aws.saving') : t('common.save') }}
              </Button>
              <Button variant="outline" @click="claudeEditing = null">{{ t('common.cancel') }}</Button>
            </div>
          </template>

          <p v-if="claudeAccountError[acc.id]" class="text-2xs text-destructive">{{ claudeAccountError[acc.id] }}</p>
        </div>
      </div>

      <!-- Auto-balance default across accounts -->
      <div v-if="claudeStore.accounts.length > 1" class="border-t border-border/60 pt-3 space-y-3">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.claude.balance.title') }}</div>
            <p class="text-2xs text-muted-foreground leading-relaxed mt-0.5">{{ t('settings.claude.balance.hint') }}</p>
          </div>
          <ToggleSwitch
            size="md"
            :checked="settings.claude_auto_balance !== 'false'"
            :label="t('settings.claude.balance.title')"
            @toggle="settings.claude_auto_balance = settings.claude_auto_balance === 'false' ? 'true' : 'false'"
          />
        </div>

        <div v-if="settings.claude_auto_balance !== 'false'" class="grid grid-cols-2 gap-3">
          <div class="space-y-1">
            <label class="text-2xs text-muted-foreground">{{ t('settings.claude.balance.cooldown') }}</label>
            <Input v-model="settings.claude_auto_balance_min_swap_minutes" type="number" min="0" max="1440" placeholder="10" />
          </div>
          <div class="space-y-1">
            <label class="text-2xs text-muted-foreground">{{ t('settings.claude.balance.threshold') }}</label>
            <Input v-model="settings.claude_auto_balance_swap_threshold_pct" type="number" min="0" max="100" placeholder="10" />
          </div>
        </div>

        <Banner v-if="showBalanceSuggestion" tone="info" :icon="ArrowRightLeft">
          {{ t('settings.claude.balance.suggest', { account: balanceRecommendedLabel }) }}
          <template v-if="settings.claude_auto_balance === 'false'" #actions>
            <Button
              size="xs"
              variant="outline"
              @click="handleSetDefaultClaude(claudeStore.balance!.recommended_id!)"
            >
              {{ t('settings.claude.setDefault') }}
            </Button>
          </template>
        </Banner>
      </div>

      <!-- Add account -->
      <div class="border-t border-border/60 pt-3 space-y-2">
        <div class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.claude.addAccount') }}</div>
        <p class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.claude.addHint') }}</p>
        <Input
          v-model="claudeNewLabel"
          size="sm"
          :placeholder="t('settings.claude.labelPlaceholder')"
          @keyup.enter="handleAddClaudeAccount"
        />
        <div class="flex justify-end">
          <Button
            :disabled="!claudeNewLabel.trim() || claudeAdding"
            @click="handleAddClaudeAccount"
          >
            <Plus class="h-3.5 w-3.5" :stroke-width="2" />
            {{ claudeAdding ? t('settings.aws.adding') : t('common.add') }}
          </Button>
        </div>
        <p v-if="claudeAddError" class="text-2xs text-destructive">{{ claudeAddError }}</p>
      </div>
  </Card>
</template>
