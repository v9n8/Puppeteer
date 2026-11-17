<script setup lang="ts">
// "New flag" / "Edit flag" dialog (V3-20). `flag` null creates. Settings › Flag uses it
// for the shared flags (`projectId` null); Project › Flag (V3-21) passes the project
// and its name, which adds the "only in this project" line.
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Input, Modal } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import { PROJECT_COLORS, swatchClass, type ProjectColor } from '@/lib/projectAppearance'
import { backendNameErrorKey, validateFlagName } from '@/lib/flagEditor'
import { FLAG_NAME_MAX, flagLabel, type SessionFlag } from '@/lib/sessionFlags'

const props = defineProps<{
  open: boolean
  flag: SessionFlag | null
  /** The flags of the same scope, the edited one included (for the duplicate check). */
  flags: SessionFlag[]
  projectId?: string | null
  projectName?: string
}>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const { toast } = useToast()
const store = useSessionFlagsStore()

const name = ref('')
const color = ref<ProjectColor>('info')
const saving = ref(false)
/** A refusal the backend gave for the name; cleared when the name changes. */
const backendError = ref<string | null>(null)
const body = ref<HTMLElement | null>(null)

watch(
  () => props.open,
  async (open) => {
    if (!open) return
    name.value = props.flag ? flagLabel(props.flag, t) : ''
    // A project's own flag starts Gray; a shared one Blue.
    color.value = props.flag?.color ?? (props.projectId ? 'neutral' : 'info')
    backendError.value = null
    await nextTick()
    body.value?.querySelector('input')?.focus()
  },
  { immediate: true },
)
watch(name, () => { backendError.value = null })

const trimmed = computed(() => name.value.trim())
const count = computed(() => [...name.value.trim()].length)
const others = computed(() => props.flags.filter((f) => f.id !== props.flag?.id).map((f) => ({ ...f, label: flagLabel(f, t) })))
const nameError = computed(() => validateFlagName(name.value, others.value))
// "Required" is not shown while the field is still empty: the button being off says it.
const shownError = computed(() => {
  if (backendError.value) return backendError.value
  return nameError.value && trimmed.value ? t(nameError.value, { max: FLAG_NAME_MAX }) : null
})
const previewName = computed(() => trimmed.value || t('sessionFlags.editor.previewEmpty'))

async function save() {
  if (nameError.value || saving.value) return
  saving.value = true
  try {
    if (props.flag) {
      // An untouched default keeps its stored name, so it goes on following the UI language.
      const unchanged = props.flag.stock_name && trimmed.value === flagLabel(props.flag, t)
      const nextName = unchanged || trimmed.value === props.flag.name ? undefined : trimmed.value
      const nextColor = color.value === props.flag.color ? undefined : color.value
      if (nextName !== undefined || nextColor !== undefined) await store.update(props.flag, { name: nextName, color: nextColor })
    } else {
      await store.create(props.projectId ?? null, trimmed.value, color.value)
    }
    emit('close')
  } catch (e) {
    const message = String(e)
    const key = backendNameErrorKey(message)
    if (key) backendError.value = t(key, { max: FLAG_NAME_MAX })
    else toast.error(message)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <Modal :open="open" size="sm" :title="t(flag ? 'sessionFlags.editor.editTitle' : 'sessionFlags.editor.newTitle')" @close="emit('close')">
    <form ref="body" class="space-y-4 p-4" @submit.prevent="save">
      <p v-if="projectName" class="text-2xs text-muted-foreground">
        {{ t('sessionFlags.editor.projectOnly', { name: projectName }) }}
      </p>

      <label class="block space-y-1">
        <span class="flex items-center justify-between text-xs text-muted-foreground">
          <span>{{ t('sessionFlags.editor.name') }}</span>
          <span class="tabular-nums" :class="count > FLAG_NAME_MAX && 'text-destructive'">{{ count }} / {{ FLAG_NAME_MAX }}</span>
        </span>
        <Input
          v-model="name"
          class="w-full"
          :placeholder="t('sessionFlags.editor.namePlaceholder')"
          :aria-invalid="!!shownError"
        />
        <span v-if="shownError" class="block text-xs text-destructive" role="alert">{{ shownError }}</span>
      </label>

      <div class="space-y-1.5">
        <span class="block text-xs text-muted-foreground">{{ t('sessionFlags.editor.color') }}</span>
        <div class="flex items-center gap-2">
          <button
            v-for="c in PROJECT_COLORS"
            :key="c"
            type="button"
            class="h-7 w-7 rounded-full border-2 transition-transform cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :class="[color === c ? 'border-foreground scale-110' : 'border-transparent', swatchClass(c)]"
            :title="t(`projectDetail.colors.${c}`)"
            :aria-label="t(`projectDetail.colors.${c}`)"
            :aria-pressed="color === c"
            @click="color = c"
          />
        </div>
      </div>

      <div class="flex items-center gap-2 text-xs text-muted-foreground">
        <span>{{ t('sessionFlags.editor.preview') }}</span>
        <span class="inline-flex min-w-0 items-center gap-1.5 rounded-full border border-border bg-muted/40 px-2 py-0.5 text-foreground">
          <span class="h-2 w-2 shrink-0 rounded-full" :class="swatchClass(color)" />
          <span class="truncate">{{ previewName }}</span>
        </span>
      </div>

      <p v-if="flag && flag.run_count > 0" class="text-xs text-muted-foreground">
        {{ t('sessionFlags.editor.runsFollow', { n: flag.run_count }) }}
      </p>

      <!-- Enter in the name field submits the form: the button below is the form's submit -->
      <div class="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" @click="emit('close')">{{ t('common.cancel') }}</Button>
        <Button type="submit" size="sm" :disabled="!!nameError || saving">
          {{ t(flag ? 'common.save' : projectId ? 'sessionFlags.editor.addProjectFlag' : 'common.add') }}
        </Button>
      </div>
    </form>
  </Modal>
</template>
