<script setup lang="ts">
// The 12-colour picker for flags and projects (DS-4): a 6 × 2 grid in colour-wheel
// order, the same in the flag dialog, the tab menu and a flag's colour popover.
import { useI18n } from 'vue-i18n'
import { Check } from 'lucide-vue-next'
import { PROJECT_COLORS, swatchClass, type ProjectColor } from '@/lib/projectAppearance'

defineProps<{
  /** The colour shown as picked; null = none. */
  selected: ProjectColor | null
  /** Extra tooltip text after the colour name (e.g. the other flags on it). */
  hint?: (color: ProjectColor) => string
}>()
const emit = defineEmits<{
  select: [color: ProjectColor]
  /** The colour under the pointer / keyboard focus; null when it leaves the grid. */
  active: [color: ProjectColor | null]
}>()

const { t } = useI18n()
const label = (c: ProjectColor) => t(`projectDetail.colors.${c}`)
</script>

<template>
  <div class="grid w-fit grid-cols-6 gap-1" @pointerleave="emit('active', null)" @focusout="emit('active', null)">
    <button
      v-for="c in PROJECT_COLORS"
      :key="c"
      type="button"
      class="relative flex h-7 w-7 items-center justify-center rounded-full transition-colors cursor-pointer hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :title="hint?.(c) ? `${label(c)} · ${hint(c)}` : label(c)"
      :aria-label="label(c)"
      :aria-pressed="selected === c"
      :data-color="c"
      @click="emit('select', c)"
      @pointerenter="emit('active', c)"
      @focus="emit('active', c)"
    >
      <span
        class="flex h-5 w-5 items-center justify-center rounded-full"
        :class="[swatchClass(c), selected === c && 'outline-2 outline-offset-2 outline-foreground']"
      >
        <!-- The page colour reads on every dot: dark dots in light mode, light ones in dark. -->
        <Check v-if="selected === c" class="h-3 w-3 text-background" :stroke-width="3" />
      </span>
    </button>
  </div>
</template>
