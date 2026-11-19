<script setup lang="ts">
// Colour palette of one flag, floating at its row (V3-20; Project › Flag reuses it).
// Picking a colour saves at once. A colour another flag uses shows that flag's name
// (line under the grid, for the colour pointed at or else the flag's own): two flags
// may share a colour, the name tells them apart.
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import ColorSwatchGrid from '@/components/ColorSwatchGrid.vue'
import { projectColor, type ProjectColor } from '@/lib/projectAppearance'
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
/** The colour under the pointer / focus; the line under the grid describes it. */
const active = ref<ProjectColor | null>(null)
const shown = computed(() => active.value ?? projectColor(props.flag.color))

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
    class="absolute left-3 top-full z-40 mt-1 w-fit rounded-lg border border-popover-border bg-popover p-2 shadow-xl shadow-black/20"
  >
    <!-- w-0 min-w-full: text lines take the grid's width instead of widening the popover. -->
    <p class="w-0 min-w-full truncate px-1 pb-1.5 text-xs font-semibold text-foreground">
      {{ t('sessionFlags.color.title', { name: flagLabel(flag, t) }) }}
    </p>
    <ColorSwatchGrid
      :selected="projectColor(flag.color)"
      :hint="sharedWith"
      @select="emit('select', $event)"
      @active="active = $event"
    />
    <p v-if="shown" class="flex w-0 min-w-full items-center gap-2 px-1 pt-1.5 text-xs" data-color-info>
      <span class="shrink-0 text-foreground">{{ t(`projectDetail.colors.${shown}`) }}</span>
      <span v-if="sharedWith(shown)" class="ml-auto min-w-0 truncate text-2xs text-muted-foreground" :title="sharedWith(shown)">
        {{ sharedWith(shown) }}
      </span>
    </p>
  </div>
</template>
