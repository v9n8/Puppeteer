<script setup lang="ts">
// Settings → "Engines & accounts" → Accounts (UX-42): one list of the Claude
// (CLAUDE_CONFIG_DIR) and Codex (CODEX_HOME) accounts, a badge for the kind on
// each row (lib/engineAccounts), and one "Add account" button for both. The
// Claude auto-balancer stays below the list. SettingsView fetches both account
// stores before showing the sections.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import { ArrowRightLeft, LogIn, Pencil, Plus, Star, Trash2, UserCircle } from 'lucide-vue-next'
import { Badge, Banner, Button, Card, InfoTip, Input, ToggleSwitch } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { useBudgetStore } from '@/stores/budget'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'
import AccountLoginProgress from './AccountLoginProgress.vue'
import AddAccountDialog from './AddAccountDialog.vue'
import { accountRows, type AccountKind, type AccountRow } from '@/lib/engineAccounts'
import { useAccountLogin } from './useAccountLogin'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const budget = useBudgetStore()
const { settings, loading } = useSettingsForm()
const claudeStore = useClaudeAccountsStore()
const codexStore = useCodexAccountsStore()
const login = useAccountLogin()

const addOpen = ref(false)

const rows = computed(() => accountRows({
  claude: claudeStore.accounts,
  claudeBudgets: claudeStore.budgets,
  codex: codexStore.accounts,
  sharedCodexLabel: t('settings.accounts.sharedCodex'),
  sharedCodexUsage: budget.codexStatus,
}))

const editing = ref<string | null>(null)
const editLabel = ref('')
const busy = ref<string | null>(null)
const errors = ref<Record<string, string>>({})
// Last validation result per account, shown inline instead of via a toast.
const validations = ref<Record<string, { logged_in: boolean; email: string | null }>>({})

function statusTone(status: string | null): string {
  switch (status) {
    case 'ready': return 'text-success'
    case 'needs_login': return 'text-warning'
    case 'error': return 'text-destructive'
    default: return 'text-muted-foreground'
  }
}

function storeOf(kind: AccountKind) {
  return kind === 'claude' ? claudeStore : codexStore
}

function startEdit(row: AccountRow) {
  editing.value = row.key
  editLabel.value = row.label
}

async function saveEdit(row: AccountRow) {
  if (!row.id) return
  busy.value = row.key
  errors.value[row.key] = ''
  try {
    await storeOf(row.kind).rename(row.id, editLabel.value.trim())
    editing.value = null
    toast.success(t('settings.github.accountUpdated'))
  } catch (e) {
    errors.value[row.key] = String(e)
  } finally {
    busy.value = null
  }
}

function signIn(row: AccountRow) {
  if (!row.id) return
  errors.value[row.key] = ''
  delete validations.value[row.key]
  void login.start(row.kind, row.id)
}

async function validate(row: AccountRow) {
  if (!row.id) return
  busy.value = row.key
  errors.value[row.key] = ''
  delete validations.value[row.key]
  login.clear(row.kind, row.id)
  try {
    validations.value[row.key] = await storeOf(row.kind).validate(row.id)
  } catch (e) {
    errors.value[row.key] = String(e)
  } finally {
    busy.value = null
  }
}

async function setDefault(row: AccountRow) {
  try {
    if (row.kind === 'claude' && row.id) {
      await claudeStore.setDefault(row.id)
      // The desktop badge follows the default account — refresh it now.
      budget.refresh()
    } else {
      await codexStore.setDefault(row.id)
    }
  } catch (e) {
    toast.error(String(e))
  }
}

async function remove(row: AccountRow) {
  if (!row.id) return
  const claude = row.kind === 'claude'
  if (!(await confirm({
    title: claude ? t('settings.claude.deleteTitle') : t('settings.accounts.deleteCodexTitle'),
    message: claude ? t('settings.claude.deleteMessage') : t('settings.accounts.deleteCodexMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  try {
    await login.stop(row.kind, row.id)
    await storeOf(row.kind).remove(row.id)
    login.clear(row.kind, row.id)
    toast.success(t('settings.github.accountDeleted'))
  } catch (e) {
    toast.error(String(e))
  }
}

// --- Claude auto-balance --------------------------------------------------

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

async function applyBalanceSuggestion() {
  const id = claudeStore.balance?.recommended_id
  if (!id) return
  try {
    await claudeStore.setDefault(id)
    budget.refresh()
  } catch (e) {
    toast.error(String(e))
  }
}

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

let unlistenDefaultChanged: UnlistenFn | null = null
onMounted(async () => {
  // The shared ~/.codex login's usage (a stored snapshot, no engine call).
  void budget.refreshCodex()
  // Auto-balancer swapped the default account → refresh the list + star + badge.
  unlistenDefaultChanged = await listen('claude_default_changed', () => claudeStore.fetch())
})
onUnmounted(() => {
  if (unlistenDefaultChanged) unlistenDefaultChanged()
})
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <UserCircle class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.engines.accounts') }}</span>
      <InfoTip :text="t('settings.accounts.intro')" />
      <Button size="xs" class="ml-auto" @click="addOpen = true">
        <Plus class="h-3 w-3" :stroke-width="2" />
        {{ t('settings.claude.addAccount') }}
      </Button>
    </template>

    <!-- Kept on screen as one line: where the credential lives matters before adding one. -->
    <div class="flex items-center gap-1.5 text-2xs text-muted-foreground">
      <span>{{ t('settings.accounts.securityShort') }}</span>
      <InfoTip :text="t('settings.accounts.securityNote')" />
    </div>

    <!-- Account list: Claude, then Codex -->
    <div v-if="rows.length" class="space-y-2">
      <div
        v-for="row in rows"
        :key="row.key"
        class="border border-border rounded-md p-3 space-y-2"
      >
        <template v-if="editing !== row.key">
          <div class="flex items-center justify-between gap-2">
            <div class="min-w-0">
              <div class="flex items-center gap-1.5 min-w-0">
                <Badge size="xs" :tone="row.kind === 'claude' ? 'primary' : 'info'">
                  {{ row.kind === 'claude' ? t('settings.accounts.typeClaude') : t('settings.accounts.typeCodex') }}
                </Badge>
                <span class="text-sm font-medium truncate" :title="row.label">{{ row.label }}</span>
                <InfoTip v-if="!row.id" :text="t('settings.accounts.sharedCodexHint')" />
                <span
                  v-if="row.isDefault"
                  class="inline-flex shrink-0 items-center gap-0.5 text-2xs text-primary font-medium"
                >
                  <Star class="h-2.5 w-2.5" :stroke-width="2" fill="currentColor" />
                  {{ t('settings.claude.default') }}
                </span>
              </div>
              <div class="text-2xs truncate" :class="statusTone(row.status)">
                <template v-if="row.status">{{ t(`settings.claude.status.${row.status}`) }}</template>
                <span v-if="login.running(row.kind, row.id ?? '')" class="text-muted-foreground"> · {{ t('settings.claude.waitingLogin') }}</span>
                <span v-if="row.detail" class="text-muted-foreground" :title="row.detail">{{ row.status ? ' · ' : '' }}{{ row.detail }}</span>
                <span
                  v-if="row.usage"
                  class="text-muted-foreground"
                  :class="{ 'text-warning': row.usage.warning, 'text-destructive': row.usage.over }"
                >
                  · {{ t('settings.claude.usagePercent', {
                    percent: row.usage.percent,
                    period: t(`settings.claude.period.${row.usage.period}`),
                  }) }}
                </span>
              </div>
            </div>
            <div class="flex items-center gap-1 shrink-0">
              <template v-if="row.id">
                <Button
                  variant="outline"
                  size="xs"
                  :disabled="busy === row.key"
                  @click="validate(row)"
                >
                  {{ busy === row.key ? '…' : t('settings.claude.validate') }}
                </Button>
                <Button
                  v-if="login.running(row.kind, row.id)"
                  variant="outline"
                  size="xs"
                  @click="login.cancel(row.kind, row.id)"
                >
                  {{ t('common.cancel') }}
                </Button>
                <Button
                  v-else-if="row.status !== 'ready'"
                  variant="outline"
                  size="xs"
                  @click="signIn(row)"
                >
                  <LogIn class="h-3 w-3" :stroke-width="1.75" />
                  {{ t('settings.claude.login') }}
                </Button>
                <Button
                  v-else
                  variant="ghost"
                  size="icon-sm"
                  :title="t('settings.accounts.signInAgain')"
                  :aria-label="t('settings.accounts.signInAgain')"
                  @click="signIn(row)"
                >
                  <LogIn class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
              </template>
              <Button
                v-if="!row.isDefault"
                variant="ghost"
                size="icon-sm"
                :title="t('settings.claude.setDefault')"
                :aria-label="t('settings.claude.setDefault')"
                @click="setDefault(row)"
              >
                <Star class="h-3.5 w-3.5" :stroke-width="1.75" />
              </Button>
              <template v-if="row.id">
                <Button
                  variant="ghost"
                  size="icon-sm"
                  :title="t('common.edit')"
                  :aria-label="t('common.edit')"
                  @click="startEdit(row)"
                >
                  <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
                <Button
                  variant="destructive-ghost"
                  size="icon-sm"
                  :title="t('common.delete')"
                  :aria-label="t('common.delete')"
                  @click="remove(row)"
                >
                  <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
              </template>
            </div>
          </div>

          <AccountLoginProgress
            v-if="row.id && login.runs[row.key] && login.runs[row.key].state !== 'cancelled'"
            :kind="row.kind"
            :run="login.runs[row.key]"
            @submit-code="login.submitCode(row.id!, $event)"
            @use-device-code="login.start(row.kind, row.id!, true)"
            @retry="login.start(row.kind, row.id!, login.runs[row.key].deviceAuth)"
          />
          <Banner v-else-if="validations[row.key]?.logged_in" tone="success">
            <div class="font-medium text-success">{{ t('settings.claude.validReady') }}</div>
            <p v-if="validations[row.key].email" class="text-muted-foreground mt-1 truncate" :title="validations[row.key].email ?? ''">
              {{ validations[row.key].email }}
            </p>
          </Banner>
          <Banner v-else-if="validations[row.key]" tone="warning">
            <span>{{ t('settings.claude.validNeedsLogin') }}</span>
          </Banner>
        </template>

        <template v-else>
          <Input
            v-model="editLabel"
            size="sm"
            :placeholder="t('settings.claude.labelPlaceholder')"
            @keyup.enter="saveEdit(row)"
          />
          <div class="flex items-center gap-2">
            <Button
              :disabled="!editLabel.trim() || busy === row.key"
              @click="saveEdit(row)"
            >
              {{ busy === row.key ? t('settings.aws.saving') : t('common.save') }}
            </Button>
            <Button variant="outline" @click="editing = null">{{ t('common.cancel') }}</Button>
          </div>
        </template>

        <p v-if="errors[row.key]" class="text-2xs text-destructive">{{ errors[row.key] }}</p>
      </div>
    </div>

    <!-- Auto-balance the Claude default across accounts -->
    <div v-if="claudeStore.accounts.length > 1" class="border-t border-border/60 pt-3 space-y-3">
      <SettingsRow :label="t('settings.claude.balance.title')" :hint="t('settings.claude.balance.hint')">
        <div class="flex min-[900px]:justify-end">
          <ToggleSwitch
            size="md"
            :checked="settings.claude_auto_balance !== 'false'"
            :label="t('settings.claude.balance.title')"
            @toggle="settings.claude_auto_balance = settings.claude_auto_balance === 'false' ? 'true' : 'false'"
          />
        </div>
      </SettingsRow>

      <template v-if="settings.claude_auto_balance !== 'false'">
        <SettingsRow :label="t('settings.claude.balance.cooldown')">
          <Input v-model="settings.claude_auto_balance_min_swap_minutes" type="number" min="0" max="1440" placeholder="10" />
        </SettingsRow>
        <SettingsRow :label="t('settings.claude.balance.threshold')">
          <Input v-model="settings.claude_auto_balance_swap_threshold_pct" type="number" min="0" max="100" placeholder="10" />
        </SettingsRow>
      </template>

      <Banner v-if="showBalanceSuggestion" tone="info" :icon="ArrowRightLeft">
        {{ t('settings.claude.balance.suggest', { account: balanceRecommendedLabel }) }}
        <template v-if="settings.claude_auto_balance === 'false'" #actions>
          <Button size="xs" variant="outline" @click="applyBalanceSuggestion">
            {{ t('settings.claude.setDefault') }}
          </Button>
        </template>
      </Banner>
    </div>
  </Card>

  <AddAccountDialog v-model:open="addOpen" :login="login" />
</template>
