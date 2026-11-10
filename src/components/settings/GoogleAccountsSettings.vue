<script setup lang="ts">
// Settings → "Google Account" (Drive + Gmail via OAuth, multi-account).
// The OAuth client (Client ID/Secret) is saved once and reused for every
// account, so adding more accounts only needs a fresh consent — no re-entry.
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  AlertTriangle, CheckCircle2, HardDrive, Loader2, LogIn, Pencil, Plus, ShieldAlert, Star, Trash2,
} from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import { Banner, Button, Card, Input } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()

interface GoogleAccount { id: string; label: string; email: string; scope: string; is_default: boolean; created_at: string }
const googleAccounts = ref<GoogleAccount[]>([])
const googleHasClient = ref(false)
const googleClientId = ref('')
const googleClientSecret = ref('')
const googleNewLabel = ref('')
const googleAdding = ref(false)
const googleError = ref<string | null>(null)
// Reveal the credential form even when creds are already saved (to change them).
const showGoogleClientForm = ref(false)
const googleEditing = ref<string | null>(null)
const googleEditLabel = ref('')

async function loadGoogleStatus() {
  try {
    googleAccounts.value = await invoke<GoogleAccount[]>('list_google_accounts')
    googleHasClient.value = (await invoke<{ has_client: boolean }>('google_client_status')).has_client
  } catch { /* leave defaults */ }
}
async function addGoogleAccount() {
  googleError.value = null
  const label = googleNewLabel.value.trim()
  if (!label) { googleError.value = t('settings.google.enterLabel'); return }
  const needCreds = !googleHasClient.value || showGoogleClientForm.value
  if (needCreds && (!googleClientId.value.trim() || !googleClientSecret.value.trim())) {
    googleError.value = t('settings.google.enterBothCreds')
    return
  }
  googleAdding.value = true
  try {
    await invoke<GoogleAccount>('add_google_account', {
      label,
      ...(needCreds ? { clientId: googleClientId.value.trim(), clientSecret: googleClientSecret.value.trim() } : {}),
    })
    googleNewLabel.value = ''
    googleClientSecret.value = ''
    showGoogleClientForm.value = false
    await loadGoogleStatus()
    toast.success(t('settings.google.accountAdded'))
  } catch (e) {
    googleError.value = String(e)
  } finally {
    googleAdding.value = false
  }
}
async function removeGoogleAccount(acc: GoogleAccount) {
  if (!(await confirm({ title: t('settings.google.removeTitle', { label: acc.label }), message: t('settings.google.removeMessage') }))) return
  try {
    await invoke('delete_google_account', { id: acc.id })
    await loadGoogleStatus()
    toast.success(t('settings.google.accountRemoved'))
  } catch (e) { googleError.value = String(e) }
}
async function saveGoogleRename(acc: GoogleAccount) {
  const label = googleEditLabel.value.trim()
  if (!label || label === acc.label) { googleEditing.value = null; return }
  try {
    await invoke('rename_google_account', { id: acc.id, label })
    googleEditing.value = null
    await loadGoogleStatus()
  } catch (e) { googleError.value = String(e) }
}
async function makeGoogleDefault(acc: GoogleAccount) {
  try {
    await invoke('set_default_google_account', { id: acc.id })
    await loadGoogleStatus()
  } catch (e) { googleError.value = String(e) }
}
async function forgetGoogleClient() {
  if (!(await confirm({ title: t('settings.google.forgetTitle'), message: t('settings.google.forgetMessage') }))) return
  try {
    await invoke('google_forget_client')
    googleClientId.value = ''
    googleClientSecret.value = ''
    showGoogleClientForm.value = false
    await loadGoogleStatus()
    toast.success(t('settings.google.credsRemoved'))
  } catch (e) { googleError.value = String(e) }
}

// --- Google validate / re-login (mirrors the AWS section) ---
interface GoogleValidation { email: string; scope: string; missing_scopes: string[] }
// Short, human-readable label for a Google scope URL (last path segment).
function scopeLabel(scope: string): string {
  return scope.split('/').pop() || scope
}
const GOOGLE_LOGIN_REQUIRED = 'GOOGLE_LOGIN_REQUIRED'
const googleValidations = ref<Record<string, GoogleValidation>>({})
const googleAccountError = ref<Record<string, string>>({})
const googleLoginNeeded = ref<Record<string, boolean>>({})
const googleBusyAccount = ref<string | null>(null)

async function handleValidateGoogle(id: string) {
  googleBusyAccount.value = id
  googleAccountError.value[id] = ''
  googleLoginNeeded.value[id] = false
  delete googleValidations.value[id]
  try {
    googleValidations.value[id] = await invoke<GoogleValidation>('validate_google_account', { id })
    await loadGoogleStatus()
  } catch (e) {
    // "Needs login" is a state (amber notice), not a hard error (red).
    if (String(e).includes(GOOGLE_LOGIN_REQUIRED)) {
      googleLoginNeeded.value[id] = true
    } else {
      googleAccountError.value[id] = String(e)
    }
  } finally {
    googleBusyAccount.value = null
  }
}

async function handleGoogleReauth(id: string) {
  googleBusyAccount.value = id
  googleAccountError.value[id] = ''
  try {
    await invoke<GoogleAccount>('reauth_google_account', { id })
    googleLoginNeeded.value[id] = false
    await loadGoogleStatus()
    // Re-validate straight away so a successful login turns the row green.
    await handleValidateGoogle(id)
  } catch (e) {
    googleAccountError.value[id] = String(e)
  } finally {
    if (googleBusyAccount.value === id) googleBusyAccount.value = null
  }
}

onMounted(loadGoogleStatus)
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <HardDrive class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.google.title') }}</span>
    </template>

    <p class="text-2xs text-muted-foreground leading-relaxed">
      {{ t('settings.google.intro') }}
    </p>

    <Banner :icon="ShieldAlert" class="leading-relaxed">
      <span class="text-muted-foreground">
        {{ t('settings.google.credsWarning') }}
      </span>
    </Banner>

    <!-- Connected accounts list -->
    <div v-if="googleAccounts.length" class="space-y-2">
      <div
        v-for="acc in googleAccounts"
        :key="acc.id"
        class="border border-border rounded-md p-3 space-y-2"
      >
        <template v-if="googleEditing !== acc.id">
          <div class="flex items-center justify-between gap-2">
            <div class="min-w-0">
              <div class="text-sm font-medium truncate flex items-center gap-1.5">
                <CheckCircle2 class="h-3.5 w-3.5 text-success" :stroke-width="1.75" />
                {{ acc.label }}
                <span v-if="acc.is_default" class="text-2xs font-medium text-primary border border-primary/40 rounded px-1">{{ t('settings.google.default') }}</span>
              </div>
              <div class="text-2xs text-muted-foreground truncate">{{ acc.email || t('settings.google.driveGmail') }}</div>
            </div>
            <div class="flex items-center gap-1 shrink-0">
              <Button
                variant="outline"
                size="xs"
                :disabled="googleBusyAccount === acc.id"
                @click="handleValidateGoogle(acc.id)"
              >
                {{ googleBusyAccount === acc.id ? '…' : t('settings.google.validate') }}
              </Button>
              <Button
                v-if="googleLoginNeeded[acc.id] || googleValidations[acc.id]?.missing_scopes?.length"
                variant="outline"
                size="xs"
                :disabled="googleBusyAccount === acc.id"
                @click="handleGoogleReauth(acc.id)"
              >
                <LogIn class="h-3 w-3" :stroke-width="1.75" />
                {{ t('settings.google.login') }}
              </Button>
              <Button v-if="!acc.is_default" size="icon-sm" variant="ghost" :title="t('settings.google.setAsDefault')" :aria-label="t('settings.google.setAsDefault')" @click="makeGoogleDefault(acc)">
                <Star class="h-3.5 w-3.5" :stroke-width="1.75" />
              </Button>
              <Button size="icon-sm" variant="ghost" :title="t('settings.google.rename')" :aria-label="t('settings.google.rename')" @click="googleEditing = acc.id; googleEditLabel = acc.label">
                <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
              </Button>
              <Button size="icon-sm" variant="destructive-ghost" :title="t('common.remove')" :aria-label="t('common.remove')" @click="removeGoogleAccount(acc)">
                <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
              </Button>
            </div>
          </div>
          <Banner
            v-if="googleValidations[acc.id] && !googleValidations[acc.id].missing_scopes?.length"
            tone="success"
          >
            <div class="font-medium text-success">{{ t('settings.google.valid') }}</div>
            <p v-if="googleValidations[acc.id].email" class="text-muted-foreground mt-1 truncate">
              {{ googleValidations[acc.id].email }}
            </p>
          </Banner>
          <Banner
            v-else-if="googleValidations[acc.id]?.missing_scopes?.length"
            tone="warning"
          >
            <span>{{ t('settings.google.missingScopes', { scopes: googleValidations[acc.id].missing_scopes.map(scopeLabel).join(', ') }) }}</span>
          </Banner>
          <Banner
            v-if="googleLoginNeeded[acc.id]"
            tone="warning"
          >
            <span>{{ googleBusyAccount === acc.id ? t('settings.google.loggingIn') : t('settings.google.loginExpired') }}</span>
          </Banner>
          <p v-if="googleAccountError[acc.id]" class="text-2xs text-destructive">{{ googleAccountError[acc.id] }}</p>
        </template>
        <template v-else>
          <div class="flex items-center gap-2">
            <Input v-model="googleEditLabel" size="sm" class="flex-1" @keyup.enter="saveGoogleRename(acc)" />
            <Button size="sm" @click="saveGoogleRename(acc)">{{ t('common.save') }}</Button>
            <Button size="sm" variant="ghost" @click="googleEditing = null">{{ t('common.cancel') }}</Button>
          </div>
        </template>
      </div>
    </div>

    <!-- Add account -->
    <div class="border border-border/60 rounded-md p-3 space-y-2.5">
      <div class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">
        {{ googleAccounts.length ? t('settings.google.addAnother') : t('settings.google.connectAccount') }}
      </div>

      <div v-if="googleHasClient && !showGoogleClientForm" class="text-2xs text-muted-foreground flex items-center gap-1.5">
        <CheckCircle2 class="h-3.5 w-3.5 text-success" :stroke-width="1.75" />
        {{ t('settings.google.credsSaved') }}
      </div>

      <!-- Credential form (first time, or when changing creds) -->
      <template v-if="!googleHasClient || showGoogleClientForm">
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.google.clientId') }}</label>
          <Input v-model="googleClientId" size="sm" placeholder="xxxxx.apps.googleusercontent.com" />
        </div>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.google.clientSecret') }}</label>
          <Input v-model="googleClientSecret" type="password" size="sm" placeholder="GOCSPX-…" />
        </div>
      </template>

      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.google.accountLabel') }}</label>
        <Input v-model="googleNewLabel" size="sm" :placeholder="t('settings.google.labelPlaceholder')" @keyup.enter="addGoogleAccount" />
      </div>

      <div v-if="googleError" class="text-2xs text-destructive flex items-start gap-1.5">
        <AlertTriangle class="h-3.5 w-3.5 shrink-0 mt-0.5" :stroke-width="1.75" />
        <span>{{ googleError }}</span>
      </div>

      <div class="flex items-center gap-2 flex-wrap">
        <Button size="sm" :disabled="googleAdding" @click="addGoogleAccount">
          <Loader2 v-if="googleAdding" class="h-3.5 w-3.5 animate-spin" :stroke-width="1.75" />
          <Plus v-else class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ googleAdding ? t('settings.google.waitingForGoogle') : t('settings.google.addAccount') }}
        </Button>
        <Button v-if="googleHasClient && !showGoogleClientForm" size="sm" variant="ghost" :disabled="googleAdding" @click="showGoogleClientForm = true">
          <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('settings.google.changeCredentials') }}
        </Button>
        <Button v-if="googleHasClient && showGoogleClientForm" size="sm" variant="ghost" :disabled="googleAdding" @click="showGoogleClientForm = false">
          {{ t('common.cancel') }}
        </Button>
        <Button v-if="googleHasClient" size="sm" variant="ghost" :disabled="googleAdding" @click="forgetGoogleClient">
          <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('settings.google.forgetSaved') }}
        </Button>
      </div>
    </div>

    <div class="rounded-md border border-border/60 bg-muted/30 p-3 space-y-2">
      <div class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.google.availableTools') }}</div>
      <div class="grid grid-cols-2 gap-x-4 gap-y-1 text-2xs text-muted-foreground">
        <span>📁 drive_list / search / read / download</span>
        <span>✏️ drive_create / upload / update / rename / move</span>
        <span>🗑️ drive_delete (permanent) · share / permissions</span>
        <span>📊 sheets_info / read / update / append</span>
        <span>📧 mail_list_messages / search / read_message / read_thread</span>
        <span>📎 mail_list_attachments / download_attachment(s)</span>
        <span>✉️ mail_send / create_draft / reply</span>
        <span>🏷️ mail_modify_labels / mark / trash / delete</span>
        <span>📅 cal_list_calendars / list_events / search_events</span>
        <span>🕒 cal_freebusy / get_event / respond_event</span>
        <span>🗓️ cal_create_event / update_event / delete_event</span>
      </div>
    </div>
  </Card>
</template>
