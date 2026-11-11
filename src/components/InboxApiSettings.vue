<script setup lang="ts">
// Settings → "Inbox API (Chrome extension)": the local HTTP endpoint the Chrome
// extension sends Slack thread links (and translation / grammar-check requests) to. Shows whether it is listening, the base URL and the
// bearer token (masked; copy / reveal / regenerate). Regenerating invalidates the
// old token immediately, so it asks first. A master switch turns the whole server
// off (nothing listens on the port).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Copy, Eye, EyeOff, Inbox, Loader2, RefreshCw } from 'lucide-vue-next'
import { Button, Card, InfoTip, ToggleSwitch } from '@/components/ui'
import SettingsRow from '@/components/settings/SettingsRow.vue'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useInboxApiStore, type InboxApiInfo } from '@/stores/inboxApi'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const store = useInboxApiStore()

const info = ref<InboxApiInfo | null>(null)
const loading = ref(true)
const error = ref<string | null>(null)
const regenerating = ref(false)
const toggling = ref(false)
const revealed = ref(false)
const copied = ref<'url' | 'token' | null>(null)

const maskedToken = computed(() => {
  const token = info.value?.token ?? ''
  if (!token) return ''
  if (revealed.value) return token
  return `${token.slice(0, 4)}${'•'.repeat(Math.max(8, token.length - 8))}${token.slice(-4)}`
})

const endpoints = computed(() => {
  const base = info.value?.baseUrl || 'http://127.0.0.1:<port>'
  return `POST ${base}/v1/slack-threads\nPOST ${base}/v1/translate\nPOST ${base}/v1/grammar`
})

async function load() {
  loading.value = true
  error.value = null
  try {
    info.value = await store.getInboxApiInfo()
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

async function toggleEnabled() {
  if (!info.value || toggling.value) return
  toggling.value = true
  try {
    info.value = await store.setInboxApiEnabled(!info.value.enabled)
  } catch (e) {
    // From a remote desktop the switch is local-only (it opens a port on the
    // controlled machine), refused as `remote_denied_local_only`. Turning it on
    // is refused as `feature_disabled` while the Integrations module is off.
    const msg = String(e)
    if (msg.includes('remote_denied_local_only')) toast.error(t('settings.inboxApi.remoteDenied'))
    else if (msg.includes('feature_disabled')) toast.error(t('settings.inboxApi.moduleOff'))
    else toast.error(msg)
  } finally {
    toggling.value = false
  }
}

async function copy(what: 'url' | 'token') {
  const text = what === 'url' ? info.value?.baseUrl : info.value?.token
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    copied.value = what
    setTimeout(() => {
      if (copied.value === what) copied.value = null
    }, 1500)
    toast.success(t('settings.inboxApi.copied'))
  } catch {
    /* clipboard unavailable */
  }
}

async function regenerate() {
  if (
    !(await confirm({
      title: t('settings.inboxApi.regenerateTitle'),
      message: t('settings.inboxApi.regenerateMessage'),
      confirmLabel: t('settings.inboxApi.regenerate'),
    }))
  ) {
    return
  }
  regenerating.value = true
  try {
    info.value = await store.regenerateInboxApiToken()
    toast.success(t('settings.inboxApi.regenerated'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    regenerating.value = false
  }
}

onMounted(load)
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Inbox class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.inboxApi.title') }}</span>
      <InfoTip :text="t('settings.inboxApi.help')" />
    </template>

    <div v-if="loading" class="h-24 animate-pulse rounded-md border border-border bg-card" />

    <p v-else-if="error" class="text-2xs text-destructive">{{ error }}</p>

    <template v-else-if="info">
      <!-- Master on/off switch (same pattern as Remote Control settings). -->
      <div
        class="flex items-center justify-between gap-4 rounded-lg border border-border/60 bg-muted/20 px-3.5 py-3"
      >
        <div class="flex min-w-0 items-center gap-1.5">
          <p class="text-sm font-medium text-foreground">{{ t('settings.inboxApi.enable') }}</p>
          <InfoTip :text="t('settings.inboxApi.enableHint')" />
        </div>
        <ToggleSwitch
          size="md"
          :checked="info.enabled"
          :label="t('settings.inboxApi.enable')"
          :loading="toggling"
          @toggle="toggleEnabled"
        />
      </div>

      <!-- Status -->
      <SettingsRow :label="t('settings.inboxApi.status')">
        <div class="flex items-center gap-2 text-xs">
          <span
            class="h-2 w-2 shrink-0 rounded-full"
            :class="info.running ? 'bg-success' : info.enabled ? 'bg-destructive' : 'bg-muted-foreground/50'"
          />
          <span>
            {{
              info.running
                ? t('settings.inboxApi.running', { port: info.port ?? '?' })
                : info.enabled
                  ? t('settings.inboxApi.stopped')
                  : t('settings.inboxApi.disabled')
            }}
          </span>
          <Button
            variant="ghost"
            size="icon-sm"
            class="ml-auto"
            :title="t('settings.inboxApi.refresh')"
            :aria-label="t('settings.inboxApi.refresh')"
            @click="load"
          >
            <RefreshCw class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </div>
      </SettingsRow>

      <template v-if="info.enabled">
        <!-- Base URL -->
        <SettingsRow :label="t('settings.inboxApi.baseUrl')">
          <div class="flex items-center gap-1.5">
            <code class="min-w-0 flex-1 truncate rounded-md border border-border/60 bg-muted/30 px-2.5 py-1.5 font-mono text-2xs">
              {{ info.baseUrl || '—' }}
            </code>
            <Button
              variant="outline"
              size="icon-sm"
              :disabled="!info.baseUrl"
              :title="t('settings.inboxApi.copy')"
              :aria-label="t('settings.inboxApi.copy')"
              @click="copy('url')"
            >
              <component :is="copied === 'url' ? Check : Copy" class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
          </div>
        </SettingsRow>

        <!-- Token -->
        <SettingsRow :label="t('settings.inboxApi.token')">
          <div class="flex items-center gap-1.5">
            <code class="min-w-0 flex-1 truncate rounded-md border border-border/60 bg-muted/30 px-2.5 py-1.5 font-mono text-2xs">
              {{ maskedToken || '—' }}
            </code>
            <Button
              variant="outline"
              size="icon-sm"
              :disabled="!info.token"
              :title="revealed ? t('settings.inboxApi.hide') : t('settings.inboxApi.reveal')"
              :aria-label="revealed ? t('settings.inboxApi.hide') : t('settings.inboxApi.reveal')"
              @click="revealed = !revealed"
            >
              <component :is="revealed ? EyeOff : Eye" class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
            <Button
              variant="outline"
              size="icon-sm"
              :disabled="!info.token"
              :title="t('settings.inboxApi.copy')"
              :aria-label="t('settings.inboxApi.copy')"
              @click="copy('token')"
            >
              <component :is="copied === 'token' ? Check : Copy" class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
          </div>
          <div class="mt-1.5 flex items-center gap-1.5">
            <Button variant="outline" size="xs" :disabled="regenerating" @click="regenerate">
              <component
                :is="regenerating ? Loader2 : RefreshCw"
                class="h-3 w-3"
                :class="{ 'animate-spin': regenerating }"
                :stroke-width="1.75"
              />
              {{ t('settings.inboxApi.regenerate') }}
            </Button>
            <InfoTip :text="t('settings.inboxApi.regenerateHint')" />
          </div>
        </SettingsRow>

        <!-- How the extension uses it -->
        <div class="rounded-md border border-border/60 bg-muted/30 p-3 space-y-1.5 text-2xs text-muted-foreground">
          <div class="font-medium uppercase tracking-wider">{{ t('settings.inboxApi.usage') }}</div>
          <code class="block whitespace-pre-wrap break-all font-mono">{{ endpoints }}
Authorization: Bearer &lt;token&gt;
Content-Type: application/json
{"url": "&lt;slack thread link&gt;"}</code>
        </div>
      </template>
    </template>
  </Card>
</template>
