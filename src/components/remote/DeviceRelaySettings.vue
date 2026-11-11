<script setup lang="ts">
// Settings → Devices, top card: what Remote Control uses — the relay (URL + auth token),
// this device's name and its master password. Another device enters that name + password
// to drive this one.
import { onMounted, ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Input, Card, InfoTip } from '@/components/ui'
import SettingsRow from '@/components/settings/SettingsRow.vue'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import { useRemoteControlStore } from '@/stores/remoteControl'
import { DEVICE_ALIAS_RE } from '@/lib/remoteLogin'
import { KeyRound } from 'lucide-vue-next'
import { errorText } from '@/lib/featureGate'

// Must match `MIN_MASTER_PASSWORD_LEN` in `src-tauri/src/secrets.rs`.
const MIN_MASTER_PASSWORD_LEN = 12

const store = useRemoteControlStore()
const { t } = useI18n()
const { toast } = useToast()
const { confirm } = useConfirm()

const relayUrl = ref('')
const authToken = ref('')
const savingConfig = ref(false)
const masterPassword = ref('')
const savingMasterPw = ref(false)
const deviceAlias = ref('')

const status = computed(() => store.status)

function fill() {
  relayUrl.value = store.status?.relay_url ?? ''
  deviceAlias.value = store.status?.device_alias ?? ''
}

onMounted(async () => {
  try {
    await store.refreshStatus()
    fill()
  } catch (e) {
    toast.error(errorText(e, t))
  }
})

async function saveConfig() {
  const url = relayUrl.value.trim()
  if (!url) {
    toast.error(t('remote.settings.toast.relayRequired'))
    return
  }
  const alias = deviceAlias.value.trim().toLowerCase()
  if (!DEVICE_ALIAS_RE.test(alias)) {
    toast.error(t('remote.settings.toast.deviceNameInvalid'))
    return
  }
  // Same rule as the backend (counts characters, like Rust's `chars()`).
  if (masterPassword.value.trim() && [...masterPassword.value].length < MIN_MASTER_PASSWORD_LEN) {
    toast.error(t('remote.settings.toast.masterTooShort', { min: MIN_MASTER_PASSWORD_LEN }))
    return
  }
  savingConfig.value = true
  try {
    await store.setConfig(url, authToken.value)
    authToken.value = '' // never keep the token around in the field
    if (alias !== (store.status?.device_alias ?? '')) await store.setDeviceAlias(alias)
    deviceAlias.value = alias
    // A non-empty field sets/updates the master password; blank leaves it
    // untouched (same "blank keeps current" convention as the auth token).
    // Clearing is done explicitly via the Clear link.
    if (masterPassword.value.trim()) {
      await store.setMasterPassword(masterPassword.value)
      masterPassword.value = ''
    }
    toast.success(t('remote.settings.toast.settingsSaved'))
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    savingConfig.value = false
  }
}

async function clearMasterPassword() {
  if (savingMasterPw.value) return
  const ok = await confirm({
    title: t('remote.settings.confirmClearMaster.title'),
    message: t('remote.settings.confirmClearMaster.message'),
    confirmLabel: t('remote.settings.confirmClearMaster.confirmLabel'),
    variant: 'destructive',
  })
  if (!ok) return
  savingMasterPw.value = true
  try {
    await store.setMasterPassword('')
    masterPassword.value = ''
    toast.success(t('remote.settings.toast.masterCleared'))
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    savingMasterPw.value = false
  }
}
</script>

<template>
  <Card>
    <template #header>
      <KeyRound class="h-4 w-4 text-primary" :stroke-width="1.75" />
      <h3 class="text-sm font-semibold">{{ t('remote.settings.identity.title') }}</h3>
      <InfoTip :text="t('remote.settings.identity.intro')" />
    </template>
    <div class="p-4 space-y-4">
      <SettingsRow :label="t('remote.settings.relayUrl')">
        <Input
          v-model="relayUrl"
          :placeholder="t('remote.settings.relayUrlPlaceholder')"
          :disabled="savingConfig"
        />
      </SettingsRow>

      <SettingsRow :label="t('remote.settings.authToken')" :hint="t('remote.settings.authTokenHint')">
        <div class="flex items-center gap-2">
          <Input
            v-model="authToken"
            type="password"
            class="flex-1"
            :placeholder="t('remote.settings.authTokenPlaceholder')"
            :disabled="savingConfig"
          />
          <span v-if="status?.has_auth_token" class="shrink-0 text-2xs text-success">{{ t('remote.settings.saved') }}</span>
        </div>
      </SettingsRow>

      <!-- Device name: picks this host's password-login code, so several
           Puppeteer hosts can share one relay (and one master password). -->
      <SettingsRow :label="t('remote.settings.deviceName')" :hint="t('remote.settings.deviceNameHint')">
        <Input
          v-model="deviceAlias"
          autocomplete="off"
          spellcheck="false"
          :placeholder="t('remote.settings.deviceNamePlaceholder')"
          :disabled="savingConfig"
          class="font-mono"
          @keyup.enter="saveConfig"
        />
      </SettingsRow>

      <!-- Master password: an optional stable code that replaces the per-join
           OTP for a single owner (falls back to OTP when unset). -->
      <SettingsRow :label="t('remote.settings.masterPassword')" :hint="t('remote.settings.masterPasswordHint')">
        <div class="flex items-center gap-2">
          <Input
            v-model="masterPassword"
            type="password"
            class="flex-1"
            :placeholder="status?.has_master_password
              ? t('remote.settings.masterPasswordReplace')
              : t('remote.settings.masterPasswordSet')"
            :disabled="savingConfig || savingMasterPw"
            @keyup.enter="saveConfig"
          />
          <template v-if="status?.has_master_password">
            <span class="shrink-0 text-2xs text-success">{{ t('remote.settings.set') }}</span>
            <button
              type="button"
              :disabled="savingMasterPw"
              class="shrink-0 text-2xs text-muted-foreground/80 hover:text-destructive transition-colors disabled:opacity-50"
              @click="clearMasterPassword"
            >
              {{ t('common.clear') }}
            </button>
          </template>
        </div>
      </SettingsRow>

      <div class="flex items-center gap-2 pt-1">
        <Button variant="outline" :disabled="savingConfig" @click="saveConfig">
          {{ t('remote.settings.saveSettings') }}
        </Button>
        <span v-if="status?.device_id" class="ml-auto text-2xs text-muted-foreground/60 font-mono">
          {{ t('remote.settings.hostLabel', { id: status.device_id.slice(0, 8) }) }}
        </span>
      </div>
    </div>
  </Card>
</template>
