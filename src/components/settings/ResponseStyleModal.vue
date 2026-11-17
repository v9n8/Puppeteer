<script setup lang="ts">
// "New style" / "Edit style" dialog of Response styles (V3-60), used by Settings
// and by the project page. `target` null creates one (in the project when
// `projectId` is set). A sample opens with its shown (translated) texts; saving
// one unchanged keeps the stored text, so it goes on following the UI language.
// A failed save stays in the dialog with the reason.
import { computed, nextTick, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Input, Modal, Textarea } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { useProjectsStore } from '@/stores/projects'
import { useResponseStylesStore } from '@/stores/responseStyles'
import { type ResponseStyle, STYLE_BODY_MAX, styleFormError, styleText } from '@/lib/responseStyles'

const props = defineProps<{
  open: boolean
  target: ResponseStyle | null
  projectId: string | null
}>()
// `saved` carries the stored row, so a caller can pick a style it just created.
const emit = defineEmits<{ 'update:open': [value: boolean]; saved: [style: ResponseStyle] }>()

const { t } = useI18n()
const { toast } = useToast()
const store = useResponseStylesStore()
const projects = useProjectsStore()

const draft = reactive({ name: '', description: '', body: '' })
const form = ref<HTMLFormElement | null>(null)
const saving = ref(false)
const saveError = ref<string | null>(null)
const formError = computed(() => styleFormError(draft))
const projectName = computed(() => projects.projects.find((p) => p.id === props.projectId)?.name ?? '')

watch(() => props.open, (open) => {
  if (!open) return
  const text = props.target ? styleText(props.target, t) : { name: '', description: '' }
  Object.assign(draft, { ...text, body: props.target?.body ?? '' })
  saveError.value = null
  nextTick(() => form.value?.querySelector('input')?.focus())
})

async function save() {
  if (formError.value || saving.value) return
  saving.value = true
  saveError.value = null
  try {
    const input = { name: draft.name, description: draft.description, body: draft.body }
    const editing = props.target
    if (editing) {
      const shown = styleText(editing, t)
      if (editing.stock_name && input.name.trim() === shown.name) input.name = editing.name
      if (editing.stock_description && input.description.trim() === shown.description) input.description = editing.description
      emit('saved', await store.update(editing.id, input))
    } else {
      emit('saved', await store.create({ projectId: props.projectId, ...input }))
    }
    toast.success(t('responseStyles.saved'))
    emit('update:open', false)
  } catch (e) {
    saveError.value = String(e)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <Modal
    :open="open"
    :title="target ? t('responseStyles.editTitle') : t('responseStyles.newTitle')"
    size="md"
    :dismiss-on-overlay="false"
    @update:open="emit('update:open', $event)"
  >
    <form ref="form" class="p-4 space-y-3" @submit.prevent="save" @keydown.meta.enter.prevent="save" @keydown.ctrl.enter.prevent="save">
      <p v-if="projectId" class="text-2xs text-muted-foreground">
        {{ t('responseStyles.projectScope', { project: projectName }) }}
      </p>
      <label class="block space-y-1">
        <span class="text-2xs text-muted-foreground">{{ t('responseStyles.name') }}</span>
        <Input v-model="draft.name" class="w-full" :placeholder="t('responseStyles.namePlaceholder')" />
      </label>
      <label class="block space-y-1">
        <span class="text-2xs text-muted-foreground">{{ t('responseStyles.description') }}</span>
        <Input v-model="draft.description" class="w-full" :placeholder="t('responseStyles.descriptionPlaceholder')" />
      </label>
      <label class="block space-y-1">
        <span class="flex items-center justify-between text-2xs text-muted-foreground">
          <span>{{ t('responseStyles.body') }}</span>
          <span>{{ t('responseStyles.bodyCount', { n: [...draft.body.trim()].length, max: STYLE_BODY_MAX }) }}</span>
        </span>
        <Textarea v-model="draft.body" rows="6" class="w-full font-mono" :placeholder="t('responseStyles.bodyPlaceholder')" />
      </label>
      <p v-if="formError && (draft.name || draft.body)" class="text-2xs text-destructive">{{ t(formError) }}</p>
      <p v-if="saveError" class="text-2xs text-destructive">{{ saveError }}</p>
    </form>
    <template #footer>
      <Button variant="outline" :disabled="saving" @click="emit('update:open', false)">{{ t('common.cancel') }}</Button>
      <Button :disabled="!!formError || saving" @click="save">{{ t('common.save') }}</Button>
    </template>
  </Modal>
</template>
