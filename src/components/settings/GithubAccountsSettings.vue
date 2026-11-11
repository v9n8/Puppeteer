<script setup lang="ts">
// Settings → Connections → GitHub: one PAT per account, validated on demand.
// Opened from the Connections list on one account (`accountId`), or with none
// on the add form. SettingsView fetches the store before showing the sections
// (the nav badge counts it too).
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, Github, Pencil, Plus, ShieldAlert, Trash2 } from 'lucide-vue-next'
import { Banner, Button, Card, InfoTip, Input } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useGithubAccountsStore, type PatValidation } from '@/stores/githubAccounts'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()

const props = defineProps<{ accountId?: string | null }>()

const ghStore = useGithubAccountsStore()
const shown = computed(() => (props.accountId ? ghStore.accounts.filter((a) => a.id === props.accountId) : []))
const newLabel = ref('')
const newPat = ref('')
const adding = ref(false)
const addError = ref<string | null>(null)
// Per-account UI state keyed by account id.
const editLabel = ref<Record<string, string>>({})
const editPat = ref<Record<string, string>>({})
const editing = ref<string | null>(null)
const validations = ref<Record<string, PatValidation>>({})
const accountError = ref<Record<string, string>>({})
const busyAccount = ref<string | null>(null)

async function handleAddAccount() {
  if (!newLabel.value.trim() || !newPat.value.trim()) return
  adding.value = true
  addError.value = null
  try {
    await ghStore.create(newLabel.value.trim(), newPat.value.trim())
    newLabel.value = ''
    newPat.value = ''
    toast.success(t('settings.github.accountAdded'))
  } catch (e) {
    addError.value = String(e)
  } finally {
    adding.value = false
  }
}

function startEdit(id: string, label: string) {
  editing.value = id
  editLabel.value[id] = label
  editPat.value[id] = ''
}

async function handleSaveEdit(id: string) {
  busyAccount.value = id
  accountError.value[id] = ''
  try {
    await ghStore.update(id, editLabel.value[id]?.trim() || '', editPat.value[id])
    editing.value = null
    toast.success(t('settings.github.accountUpdated'))
  } catch (e) {
    accountError.value[id] = String(e)
  } finally {
    busyAccount.value = null
  }
}

async function handleValidate(id: string) {
  busyAccount.value = id
  accountError.value[id] = ''
  delete validations.value[id]
  try {
    validations.value[id] = await ghStore.validate(id)
  } catch (e) {
    accountError.value[id] = String(e)
  } finally {
    busyAccount.value = null
  }
}

async function handleDeleteAccount(id: string) {
  if (!(await confirm({
    title: t('settings.github.deleteTitle'),
    message: t('settings.github.deleteMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  try {
    await ghStore.remove(id)
    toast.success(t('settings.github.accountDeleted'))
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Github class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.github.title') }}</span>
      <InfoTip v-if="!accountId" :text="t('settings.github.intro')" />
    </template>
    <template v-if="!accountId">
      <Banner :icon="ShieldAlert" class="leading-relaxed">
        <span class="text-muted-foreground">
          {{ t('settings.github.isolationWarning') }}
        </span>
      </Banner>
    </template>

      <!-- Account list -->
      <div v-if="shown.length" class="space-y-2">
        <div
          v-for="acc in shown"
          :key="acc.id"
          class="border border-border rounded-md p-3 space-y-2"
        >
          <!-- View mode -->
          <template v-if="editing !== acc.id">
            <div class="flex items-center justify-between gap-2">
              <div class="min-w-0">
                <div class="text-sm font-medium truncate">{{ acc.label }}</div>
                <div class="text-2xs text-muted-foreground truncate">
                  <span v-if="acc.username">@{{ acc.username }}</span>
                  <span v-else>{{ t('settings.github.notValidated') }}</span>
                  <span v-if="acc.scopes.length" class="ml-1">· {{ acc.scopes.join(', ') }}</span>
                </div>
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <Button
                  variant="outline"
                  size="xs"
                  :disabled="busyAccount === acc.id"
                  @click="handleValidate(acc.id)"
                >
                  {{ busyAccount === acc.id ? '…' : t('settings.github.validate') }}
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  :title="t('common.edit')"
                  :aria-label="t('common.edit')"
                  @click="startEdit(acc.id, acc.label)"
                >
                  <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
                <Button
                  variant="destructive-ghost"
                  size="icon-sm"
                  :title="t('common.delete')"
                  :aria-label="t('common.delete')"
                  @click="handleDeleteAccount(acc.id)"
                >
                  <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
              </div>
            </div>
            <Banner
              v-if="validations[acc.id]"
              tone="success"
            >
              <div class="font-medium text-success">{{ t('settings.github.valid') }}</div>
              <p v-if="validations[acc.id].username" class="text-muted-foreground mt-1 truncate">@{{ validations[acc.id].username }}</p>
              <p v-if="!validations[acc.id].has_repo_scope" class="text-warning mt-1 flex items-center gap-1">
                <AlertTriangle class="h-3 w-3" :stroke-width="1.75" />
                {{ t('settings.github.missingRepoScope') }}
              </p>
            </Banner>
          </template>

          <!-- Edit mode -->
          <template v-else>
            <Input
              v-model="editLabel[acc.id]"
              size="sm"
              :placeholder="t('settings.aws.labelPlaceholder')"
            />
            <Input
              v-model="editPat[acc.id]"
              type="password"
              size="sm"
              :placeholder="t('settings.github.patPlaceholder')"
              class="font-mono"
            />
            <div class="flex items-center gap-2">
              <Button
                :disabled="!editLabel[acc.id]?.trim() || busyAccount === acc.id"
                @click="handleSaveEdit(acc.id)"
              >
                {{ busyAccount === acc.id ? t('settings.github.saving') : t('common.save') }}
              </Button>
              <Button
                variant="outline"
                @click="editing = null"
              >
                {{ t('common.cancel') }}
              </Button>
            </div>
          </template>

          <p v-if="accountError[acc.id]" class="text-2xs text-destructive">{{ accountError[acc.id] }}</p>
        </div>
      </div>

      <!-- Add account -->
      <div v-if="!accountId" class="border-t border-border/60 pt-3 space-y-2">
        <div class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.github.addAccount') }}</div>
        <Input
          v-model="newLabel"
          size="sm"
          :placeholder="t('settings.github.labelPlaceholder')"
        />
        <div class="flex gap-2">
          <Input
            v-model="newPat"
            type="password"
            size="sm"
            placeholder="ghp_…"
            class="flex-1 font-mono"
            @keyup.enter="handleAddAccount"
          />
          <Button
            :disabled="!newLabel.trim() || !newPat.trim() || adding"
            @click="handleAddAccount"
          >
            <Plus class="h-3.5 w-3.5" :stroke-width="2" />
            {{ adding ? t('settings.github.adding') : t('common.add') }}
          </Button>
        </div>
        <p v-if="addError" class="text-2xs text-destructive">{{ addError }}</p>
      </div>
  </Card>
</template>
