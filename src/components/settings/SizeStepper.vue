<script setup lang="ts">
// A text size picked with − / +, the value in px between them. Steps through
// `sizes` (ascending); each end disables its button.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Minus, Plus } from 'lucide-vue-next'

const props = defineProps<{ sizes: readonly number[] }>()
const model = defineModel<number>({ required: true })

const { t } = useI18n()

const index = computed(() => props.sizes.indexOf(model.value))
function step(by: number) {
  const next = props.sizes[index.value + by]
  if (next !== undefined) model.value = next
}
</script>

<template>
  <div class="inline-flex items-center rounded-md border border-input bg-card">
    <button
      type="button"
      class="grid h-8 w-8 place-items-center rounded-l-md border-r border-border text-muted-foreground transition-colors cursor-pointer hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:text-border disabled:hover:bg-transparent"
      :disabled="index <= 0"
      :title="t('settings.general.sizeSmaller')"
      :aria-label="t('settings.general.sizeSmaller')"
      @click="step(-1)"
    >
      <Minus class="h-4 w-4" :stroke-width="2" />
    </button>
    <output class="w-16 text-center text-xs font-medium tabular-nums">{{ t('settings.general.fontSizePx', { size: model }) }}</output>
    <button
      type="button"
      class="grid h-8 w-8 place-items-center rounded-r-md border-l border-border text-muted-foreground transition-colors cursor-pointer hover:bg-accent hover:text-foreground disabled:cursor-not-allowed disabled:text-border disabled:hover:bg-transparent"
      :disabled="index >= sizes.length - 1"
      :title="t('settings.general.sizeLarger')"
      :aria-label="t('settings.general.sizeLarger')"
      @click="step(1)"
    >
      <Plus class="h-4 w-4" :stroke-width="2" />
    </button>
  </div>
</template>
