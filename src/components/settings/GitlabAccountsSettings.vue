<script setup lang="ts">
// Settings → "GitLab Accounts": mirror of the GitHub section, plus host + email.
// SettingsView fetches the store before showing the sections (the nav badge
// counts it too).
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Gitlab, Pencil, Plus, ShieldAlert, Trash2 } from 'lucide-vue-next'
import { Banner, Button, Card, Input } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useGitlabAccountsStore, type GitlabPatValidation } from '@/stores/gitlabAccounts'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()

const glStore = useGitlabAccountsStore()
const glNewLabel = ref('')
const glNewPat = ref('')
const glNewHost = ref('')
const glNewEmail = ref('')
const glAdding = ref(false)
const glAddError = ref<string | null>(null)
const glEditLabel = ref<Record<string, string>>({})
const glEditPat = ref<Record<string, string>>({})
const glEditHost = ref<Record<string, string>>({})
const glEditEmail = ref<Record<string, string>>({})
const glEditing = ref<string | null>(null)
const glValidations = ref<Record<string, GitlabPatValidation>>({})
const glAccountError = ref<Record<string, string>>({})
const glBusyAccount = ref<string | null>(null)

async function handleAddGitlabAccount() {
  if (!glNewLabel.value.trim() || !glNewPat.value.trim()) return
  glAdding.value = true
  glAddError.value = null
  try {
    await glStore.create(
      glNewLabel.value.trim(),
      glNewPat.value.trim(),
      glNewHost.value.trim(),
      glNewEmail.value.trim(),
    )
    glNewLabel.value = ''
    glNewPat.value = ''
    glNewHost.value = ''
    glNewEmail.value = ''
    toast.success(t('settings.github.accountAdded'))
  } catch (e) {
    glAddError.value = String(e)
  } finally {
    glAdding.value = false
  }
}

function startGitlabEdit(id: string, label: string, host: string | null, email: string | null) {
  glEditing.value = id
  glEditLabel.value[id] = label
  glEditPat.value[id] = ''
  glEditHost.value[id] = host ?? ''
  glEditEmail.value[id] = email ?? ''
}

async function handleSaveGitlabEdit(id: string) {
  glBusyAccount.value = id
  glAccountError.value[id] = ''
  try {
    await glStore.update(
      id,
      glEditLabel.value[id]?.trim() || '',
      glEditPat.value[id],
      glEditHost.value[id],
      glEditEmail.value[id],
    )
    glEditing.value = null
    toast.success(t('settings.github.accountUpdated'))
  } catch (e) {
    glAccountError.value[id] = String(e)
  } finally {
    glBusyAccount.value = null
  }
}

async function handleValidateGitlab(id: string) {
  glBusyAccount.value = id
  glAccountError.value[id] = ''
  delete glValidations.value[id]
  try {
    glValidations.value[id] = await glStore.validate(id)
  } catch (e) {
    glAccountError.value[id] = String(e)
  } finally {
    glBusyAccount.value = null
  }
}

async function handleDeleteGitlabAccount(id: string) {
  if (!(await confirm({
    title: t('settings.gitlab.deleteTitle'),
    message: t('settings.gitlab.deleteMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  try {
    await glStore.remove(id)
    toast.success(t('settings.github.accountDeleted'))
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Gitlab class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.gitlab.title') }}</span>
    </template>
      <p class="text-2xs text-muted-foreground leading-relaxed">
        {{ t('settings.gitlab.intro') }}
      </p>

      <Banner :icon="ShieldAlert" class="leading-relaxed">
        <span class="text-muted-foreground">
          {{ t('settings.gitlab.isolationWarning') }}
        </span>
      </Banner>

      <!-- Account list -->
      <div v-if="glStore.accounts.length" class="space-y-2">
        <div
          v-for="acc in glStore.accounts"
          :key="acc.id"
          class="border border-border rounded-md p-3 space-y-2"
        >
          <!-- View mode -->
          <template v-if="glEditing !== acc.id">
            <div class="flex items-center justify-between gap-2">
              <div class="min-w-0">
                <div class="text-sm font-medium truncate">{{ acc.label }}</div>
                <div class="text-2xs text-muted-foreground truncate">
                  <span v-if="acc.username">@{{ acc.username }}</span>
                  <span v-else>{{ t('settings.gitlab.notValidated') }}</span>
                  <span v-if="acc.host" class="ml-1">· {{ acc.host }}</span>
                  <span v-if="acc.email" class="ml-1">· {{ acc.email }}</span>
                </div>
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <Button
                  variant="outline"
                  size="xs"
                  :disabled="glBusyAccount === acc.id"
                  @click="handleValidateGitlab(acc.id)"
                >
                  {{ glBusyAccount === acc.id ? '…' : t('settings.gitlab.validate') }}
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  :title="t('common.edit')"
                  :aria-label="t('common.edit')"
                  @click="startGitlabEdit(acc.id, acc.label, acc.host, acc.email)"
                >
                  <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
                <Button
                  variant="destructive-ghost"
                  size="icon-sm"
                  :title="t('common.delete')"
                  :aria-label="t('common.delete')"
                  @click="handleDeleteGitlabAccount(acc.id)"
                >
                  <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
              </div>
            </div>
            <Banner
              v-if="glValidations[acc.id]"
              tone="success"
            >
              <div class="font-medium text-success">{{ t('settings.gitlab.valid') }}</div>
              <p v-if="glValidations[acc.id].username || glValidations[acc.id].email" class="text-muted-foreground mt-1 truncate">
                <template v-if="glValidations[acc.id].username">@{{ glValidations[acc.id].username }}</template>
                <template v-if="glValidations[acc.id].email"> · {{ glValidations[acc.id].email }}</template>
              </p>
            </Banner>
          </template>

          <!-- Edit mode -->
          <template v-else>
            <Input
              v-model="glEditLabel[acc.id]"
              size="sm"
              :placeholder="t('settings.aws.labelPlaceholder')"
            />
            <Input
              v-model="glEditHost[acc.id]"
              size="sm"
              placeholder="https://gitlab.com"
            />
            <Input
              v-model="glEditEmail[acc.id]"
              size="sm"
              :placeholder="t('settings.gitlab.commitEmailPlaceholder')"
            />
            <Input
              v-model="glEditPat[acc.id]"
              type="password"
              size="sm"
              :placeholder="t('settings.gitlab.patPlaceholder')"
              class="font-mono"
            />
            <div class="flex items-center gap-2">
              <Button
                :disabled="!glEditLabel[acc.id]?.trim() || glBusyAccount === acc.id"
                @click="handleSaveGitlabEdit(acc.id)"
              >
                {{ glBusyAccount === acc.id ? t('settings.gitlab.saving') : t('common.save') }}
              </Button>
              <Button
                variant="outline"
                @click="glEditing = null"
              >
                {{ t('common.cancel') }}
              </Button>
            </div>
          </template>

          <p v-if="glAccountError[acc.id]" class="text-2xs text-destructive">{{ glAccountError[acc.id] }}</p>
        </div>
      </div>

      <!-- Add account -->
      <div class="border-t border-border/60 pt-3 space-y-2">
        <div class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.gitlab.addAccount') }}</div>
        <Input
          v-model="glNewLabel"
          size="sm"
          :placeholder="t('settings.gitlab.labelPlaceholder')"
        />
        <Input
          v-model="glNewHost"
          size="sm"
          placeholder="https://gitlab.com"
        />
        <Input
          v-model="glNewEmail"
          size="sm"
          :placeholder="t('settings.gitlab.commitEmailPlaceholder')"
        />
        <div class="flex gap-2">
          <Input
            v-model="glNewPat"
            type="password"
            size="sm"
            placeholder="glpat-…"
            class="flex-1 font-mono"
            @keyup.enter="handleAddGitlabAccount"
          />
          <Button
            :disabled="!glNewLabel.trim() || !glNewPat.trim() || glAdding"
            @click="handleAddGitlabAccount"
          >
            <Plus class="h-3.5 w-3.5" :stroke-width="2" />
            {{ glAdding ? t('settings.gitlab.adding') : t('common.add') }}
          </Button>
        </div>
        <p v-if="glAddError" class="text-2xs text-destructive">{{ glAddError }}</p>
      </div>
  </Card>
</template>
