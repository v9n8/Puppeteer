<script setup lang="ts">
// "Add account" (Settings → Engines & accounts): one dialog for both kinds, in
// three steps — pick Claude (Pro/Max) or ChatGPT (Codex), name it, sign in.
// The account is created when the sign-in starts; closing the dialog before it
// has signed in removes it again, so a cancelled or failed attempt leaves no
// empty account behind.
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Bot, Sparkles } from 'lucide-vue-next'
import { Button, InfoTip, Input, Modal } from '@/components/ui'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import AccountLoginProgress from './AccountLoginProgress.vue'
import { accountKey, type AccountKind } from '@/lib/engineAccounts'
import type { AccountLogin } from './useAccountLogin'

const props = defineProps<{
  open: boolean
  login: AccountLogin
}>()
const emit = defineEmits<{ 'update:open': [value: boolean] }>()

const { t } = useI18n()
const claudeStore = useClaudeAccountsStore()
const codexStore = useCodexAccountsStore()

const step = ref<1 | 2 | 3>(1)
const kind = ref<AccountKind>('claude')
const label = ref('')
const creating = ref(false)
const createError = ref<string | null>(null)
/** The account this dialog created; null until step 3. */
const accountId = ref<string | null>(null)
const closing = ref(false)
const labelInput = ref<{ $el?: HTMLInputElement } | null>(null)

const KINDS = computed(() => [
  { id: 'claude' as const, icon: Sparkles, title: t('settings.accounts.add.claude'), desc: t('settings.accounts.add.claudeDesc') },
  { id: 'codex' as const, icon: Bot, title: t('settings.accounts.add.codex'), desc: t('settings.accounts.add.codexDesc') },
])
const STEPS = computed(() => [
  t('settings.accounts.add.stepType'),
  t('settings.accounts.add.stepName'),
  t('settings.accounts.add.stepSignIn'),
])

const run = computed(() => (accountId.value ? props.login.runs[accountKey(kind.value, accountId.value)] ?? null : null))
const signedIn = computed(() => run.value?.state === 'done')

watch(() => props.open, (open) => {
  if (!open) return
  step.value = 1
  kind.value = 'claude'
  label.value = ''
  createError.value = null
  accountId.value = null
})

async function toName() {
  step.value = 2
  await nextTick()
  labelInput.value?.$el?.focus?.()
}

async function createAndSignIn() {
  const name = label.value.trim()
  if (!name || creating.value) return
  creating.value = true
  createError.value = null
  try {
    const acc = kind.value === 'claude' ? await claudeStore.create(name) : await codexStore.create(name)
    accountId.value = acc.id
    step.value = 3
    void props.login.start(kind.value, acc.id)
  } catch (e) {
    createError.value = String(e)
  } finally {
    creating.value = false
  }
}

function restart(deviceAuth: boolean) {
  if (accountId.value) void props.login.start(kind.value, accountId.value, deviceAuth)
}

async function close() {
  if (closing.value) return
  const id = accountId.value
  if (id && !signedIn.value) {
    closing.value = true
    try {
      await props.login.stop(kind.value, id)
      await (kind.value === 'claude' ? claudeStore.remove(id) : codexStore.remove(id))
    } catch {
      // Left in the list as "Needs login": it can be signed in or deleted there.
    } finally {
      closing.value = false
    }
  }
  if (id) props.login.clear(kind.value, id)
  emit('update:open', false)
}
</script>

<template>
  <Modal
    :open="open"
    :title="t('settings.accounts.add.title')"
    size="sm"
    :dismiss-on-overlay="false"
    @close="close"
  >
    <div class="p-4 space-y-4">
      <!-- Step indicator -->
      <ol class="flex items-center gap-2 text-2xs">
        <li
          v-for="(name, i) in STEPS"
          :key="name"
          class="flex items-center gap-1.5"
          :class="i + 1 === step ? 'text-foreground font-medium' : 'text-muted-foreground'"
        >
          <span
            class="inline-flex h-4 w-4 items-center justify-center rounded-full border text-[10px] tabular-nums"
            :class="i + 1 <= step ? 'border-primary bg-primary text-primary-foreground' : 'border-border'"
          >{{ i + 1 }}</span>
          {{ name }}
          <span v-if="i < STEPS.length - 1" class="text-muted-foreground/50">·</span>
        </li>
      </ol>

      <!-- 1. Type -->
      <div v-if="step === 1" class="grid grid-cols-2 gap-2" role="radiogroup" :aria-label="t('settings.accounts.add.stepType')">
        <button
          v-for="k in KINDS"
          :key="k.id"
          type="button"
          role="radio"
          :aria-checked="kind === k.id"
          class="flex flex-col items-start gap-1 rounded-md border p-3 text-left transition-colors cursor-pointer"
          :class="kind === k.id ? 'border-primary bg-primary/5' : 'border-border hover:bg-accent/50'"
          @click="kind = k.id"
          @dblclick="toName"
        >
          <component :is="k.icon" class="h-4 w-4 text-muted-foreground" :stroke-width="1.75" />
          <span class="text-xs font-medium">{{ k.title }}</span>
          <span class="text-2xs text-muted-foreground leading-relaxed">{{ k.desc }}</span>
        </button>
      </div>

      <!-- 2. Name -->
      <div v-else-if="step === 2" class="space-y-1.5">
        <div class="flex items-center gap-1.5">
          <label for="add-account-label" class="text-xs font-medium">{{ t('settings.accounts.add.stepName') }}</label>
          <InfoTip :text="kind === 'claude' ? t('settings.accounts.add.claudeHint') : t('settings.accounts.add.codexHint')" />
        </div>
        <Input
          id="add-account-label"
          ref="labelInput"
          v-model="label"
          size="sm"
          :placeholder="t('settings.claude.labelPlaceholder')"
          @keyup.enter="createAndSignIn"
        />
        <p v-if="createError" class="text-2xs text-destructive">{{ createError }}</p>
      </div>

      <!-- 3. Sign in -->
      <div v-else class="space-y-2">
        <div class="text-xs">
          <span class="font-medium">{{ label.trim() }}</span>
          <span class="text-muted-foreground"> · {{ kind === 'claude' ? t('settings.accounts.add.claude') : t('settings.accounts.add.codex') }}</span>
        </div>
        <AccountLoginProgress
          v-if="run"
          :kind="kind"
          :run="run"
          @submit-code="accountId && login.submitCode(accountId, $event)"
          @use-device-code="restart(true)"
          @retry="restart(run.deviceAuth)"
        />
        <p v-if="!signedIn" class="text-2xs text-muted-foreground">{{ t('settings.accounts.add.cancelNote') }}</p>
      </div>
    </div>

    <template #footer>
      <template v-if="step === 1">
        <Button variant="outline" @click="close">{{ t('common.cancel') }}</Button>
        <Button @click="toName">{{ t('settings.accounts.add.next') }}</Button>
      </template>
      <template v-else-if="step === 2">
        <Button variant="outline" :disabled="creating" @click="step = 1">{{ t('common.back') }}</Button>
        <Button :disabled="!label.trim() || creating" @click="createAndSignIn">
          {{ creating ? t('settings.aws.adding') : t('settings.accounts.add.signIn') }}
        </Button>
      </template>
      <template v-else>
        <Button v-if="signedIn" @click="close">{{ t('settings.accounts.add.done') }}</Button>
        <Button v-else variant="outline" :disabled="closing" @click="close">{{ t('common.cancel') }}</Button>
      </template>
    </template>
  </Modal>
</template>
