<script setup lang="ts">
// The steps of one account sign-in (useAccountLogin), shown in the "Add
// account" dialog and under an account row: the sign-in page (opened by the
// CLI, or by hand), the wait, and the fallback the engine offers — Claude's
// paste-a-code field, Codex's device code.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import { Check, Circle, Copy, Loader2 } from 'lucide-vue-next'
import { Banner, Button, Input } from '@/components/ui'
import { loginSteps, type AccountKind } from '@/lib/engineAccounts'
import type { LoginRun } from './useAccountLogin'

const props = defineProps<{
  kind: AccountKind
  run: LoginRun
}>()
const emit = defineEmits<{
  submitCode: [code: string]
  useDeviceCode: []
  retry: []
}>()

const { t } = useI18n()
const code = ref('')
const copied = ref(false)

const steps = computed(() => {
  const labels = {
    open: t('settings.accounts.login.openPage'),
    code: props.run.code ? t('settings.accounts.login.deviceCode') : t('settings.accounts.login.waitingCode'),
    wait: t('settings.claude.loginWaitingHint'),
  }
  return loginSteps(props.run).map((s) => ({ ...s, label: labels[s.key] }))
})

function openPage() {
  if (props.run.url) void openUrl(props.run.url).catch(() => {})
}

function submit() {
  if (!code.value.trim()) return
  emit('submitCode', code.value)
  code.value = ''
}

async function copyCode() {
  if (!props.run.code) return
  try {
    await navigator.clipboard.writeText(props.run.code)
    copied.value = true
    setTimeout(() => { copied.value = false }, 1500)
  } catch {
    /* clipboard unavailable: the code stays on screen */
  }
}
</script>

<template>
  <div class="space-y-2 text-2xs">
    <template v-if="run.state === 'running'">
      <ol class="space-y-1.5">
        <li v-for="s in steps" :key="s.key" class="flex items-center gap-2">
          <Check v-if="s.state === 'done'" class="h-3 w-3 shrink-0 text-success" :stroke-width="2" />
          <Loader2 v-else-if="s.state === 'active'" class="h-3 w-3 shrink-0 animate-spin text-muted-foreground" :stroke-width="2" />
          <Circle v-else class="h-3 w-3 shrink-0 text-muted-foreground/50" :stroke-width="2" />
          <span :class="s.state === 'pending' ? 'text-muted-foreground/70' : 'text-foreground'">{{ s.label }}</span>
          <button
            v-if="s.key === 'open' && run.url"
            type="button"
            class="text-primary hover:underline cursor-pointer"
            @click="openPage"
          >
            {{ t('settings.accounts.login.openAgain') }}
          </button>
        </li>
      </ol>

      <!-- Codex device code: typed on the page, so show it big and copyable -->
      <div v-if="run.deviceAuth && run.code" class="flex items-center gap-2 pl-5">
        <code class="rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-sm tracking-wider select-all">{{ run.code }}</code>
        <Button
          variant="ghost"
          size="icon-sm"
          :title="t('settings.accounts.login.copyCode')"
          :aria-label="t('settings.accounts.login.copyCode')"
          @click="copyCode"
        >
          <component :is="copied ? Check : Copy" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
      </div>

      <!-- Claude: the manual page shows a code when the browser can't reach the CLI -->
      <div v-if="kind === 'claude' && run.url" class="flex items-center gap-1.5 pl-5">
        <Input
          v-model="code"
          size="sm"
          class="flex-1"
          :placeholder="t('settings.accounts.login.pasteCode')"
          @keyup.enter="submit"
        />
        <Button variant="outline" size="sm" :disabled="!code.trim()" @click="submit">
          {{ t('settings.claude.loginSubmitCode') }}
        </Button>
      </div>

      <button
        v-if="kind === 'codex' && !run.deviceAuth"
        type="button"
        class="pl-5 text-primary hover:underline cursor-pointer text-left"
        @click="emit('useDeviceCode')"
      >
        {{ t('settings.accounts.login.useDeviceCode') }}
      </button>

      <p v-if="run.error" class="pl-5 text-destructive">{{ run.error }}</p>
    </template>

    <Banner v-else-if="run.state === 'done'" tone="success" live>
      <span class="font-medium text-success">
        {{ run.email ? t('settings.accounts.login.signedInAs', { email: run.email }) : t('settings.accounts.login.signedIn') }}
      </span>
    </Banner>

    <Banner v-else-if="run.state === 'failed'" tone="destructive" live>
      <span class="break-words">{{ run.error }}</span>
      <template #actions>
        <Button size="xs" variant="outline" @click="emit('retry')">{{ t('settings.accounts.add.retry') }}</Button>
      </template>
    </Banner>
  </div>
</template>
