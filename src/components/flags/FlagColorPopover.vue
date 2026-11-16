<script setup lang="ts">
// Colour palette of one flag, floating at its row (V3-20; Project › Flag reuses it).
// Picking a colour saves at once. A colour another flag uses shows that flag's name:
// two flags may share a colour, the name tells them apart.
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check } from 'lucide-vue-next'
import { PROJECT_COLORS, swatchClass, type ProjectColor } from '@/lib/projectAppearance'
import { colorUsers } from '@/lib/flagEditor'
import { flagLabel, type SessionFlag } from '@/lib/sessionFlags'

const props = defineProps<{
  flag: SessionFlag
  /** The flags of the same scope, the edited one included. */
  flags: SessionFlag[]
}>()
const emit = defineEmits<{ select: [color: ProjectColor]; close: [] }>()

const { t } = useI18n()
const root = ref<HTMLElement | null>(null)

function sharedWith(color: ProjectColor): string {
  return colorUsers(props.flags, color, props.flag.id).map((f) => flagLabel(f, t)).join(', ')
}

function onPointerDown(e: PointerEvent) {
  const target = e.target as Element
  // The dot that opened it toggles it itself.
  if (root.value && !root.value.contains(target) && !target.closest('[aria-expanded="true"]')) emit('close')
}

function onKey(e: KeyboardEvent) {
  if (e.key !== 'Escape') return
  e.stopPropagation()
  emit('close')
}

onMounted(() => {
  window.addEventListener('pointerdown', onPointerDown, true)
  window.addEventListener('keydown', onKey, true)
  root.value?.querySelector<HTMLElement>('button[aria-pressed="true"]')?.focus()
})
onBeforeUnmount(() => {
  window.removeEventListener('pointerdown', onPointerDown, true)
  window.removeEventListener('keydown', onKey, true)
})
</script>

<template>
  <div
    ref="root"
    role="dialog"
    :aria-label="t('sessionFlags.color.title', { name: flagLabel(flag, t) })"
    class="absolute left-3 top-full z-40 mt-1 w-64 max-w-[calc(100vw-2rem)] rounded-lg border border-popover-border bg-popover p-2 shadow-xl shadow-black/20"
  >
    <p class="truncate px-1.5 pb-1.5 text-xs font-semibold text-foreground">
      {{ t('sessionFlags.color.title', { name: flagLabel(flag, t) }) }}
    </p>
    <button
      v-for="color in PROJECT_COLORS"
      :key="color"
      type="button"
      class="flex h-8 w-full items-center gap-2 rounded-md px-1.5 text-left text-xs transition-colors cursor-pointer hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :aria-pressed="flag.color === color"
      @click="emit('select', color)"
    >
      <span class="h-4 w-4 shrink-0 rounded-full" :class="swatchClass(color)" />
      <span class="text-foreground">{{ t(`projectDetail.colors.${color}`) }}</span>
      <span v-if="sharedWith(color)" class="ml-auto min-w-0 truncate text-2xs text-muted-foreground" :title="sharedWith(color)">
        {{ sharedWith(color) }}
      </span>
      <Check v-if="flag.color === color" class="h-3.5 w-3.5 shrink-0 text-primary" :class="sharedWith(color) ? '' : 'ml-auto'" :stroke-width="2.25" />
    </button>
    <p class="px-1.5 pt-1.5 text-2xs text-muted-foreground">{{ t('sessionFlags.color.footer') }}</p>
  </div>
</template>
