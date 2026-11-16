<script setup lang="ts">
// "Rename account" (Settings → Engines & accounts, V3-60): the label of a Claude
// or Codex account. Enter saves, Esc cancels (Modal); an empty label cannot be
// saved. The caller saves and passes back `error` / `busy`, so a failed save
// keeps the dialog open with the reason.
import { nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Input, Modal } from '@/components/ui'

const props = defineProps<{
  open: boolean
  label: string
  busy?: boolean
  error?: string | null
}>()
const emit = defineEmits<{ 'update:open': [value: boolean]; save: [label: string] }>()

const { t } = useI18n()
const draft = ref('')
const input = ref<{ $el?: HTMLInputElement } | null>(null)

watch(() => props.open, async (open) => {
  if (!open) return
  draft.value = props.label
  await nextTick()
  input.value?.$el?.focus?.()
})

function submit() {
  const label = draft.value.trim()
  if (label && !props.busy) emit('save', label)
}
</script>

<template>
  <Modal :open="open" :title="t('settings.accounts.rename.title')" size="sm" @update:open="emit('update:open', $event)">
    <form class="p-4 space-y-1.5" @submit.prevent="submit">
      <label for="rename-account-label" class="text-xs font-medium">{{ t('settings.accounts.rename.label') }}</label>
      <Input
        id="rename-account-label"
        ref="input"
        v-model="draft"
        size="sm"
        :placeholder="t('settings.claude.labelPlaceholder')"
      />
      <p class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.accounts.rename.note') }}</p>
      <p v-if="error" class="text-2xs text-destructive">{{ error }}</p>
    </form>
    <template #footer>
      <Button variant="outline" :disabled="busy" @click="emit('update:open', false)">{{ t('common.cancel') }}</Button>
      <Button :disabled="!draft.trim() || busy" @click="submit">
        {{ busy ? t('settings.aws.saving') : t('common.save') }}
      </Button>
    </template>
  </Modal>
</template>
