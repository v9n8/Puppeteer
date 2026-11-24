<script setup lang="ts">
// One line of Settings › Flag (V3-20): drag handle, colour dot (opens the palette), name,
// how many Sessions carry it, Edit, Delete. Drag and keyboard moves are the list's job:
// the row only reports the handle's events.
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { GripVertical, Pencil, Trash2 } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import FlagColorPopover from '@/components/flags/FlagColorPopover.vue'
import { swatchClass, type ProjectColor } from '@/lib/projectAppearance'
import { flagLabel, type SessionFlag } from '@/lib/sessionFlags'

defineProps<{
  flag: SessionFlag
  index: number
  flags: SessionFlag[]
  /** Being dragged with the pointer. */
  dragging: boolean
  /** Lifted with Space: arrows move it. */
  lifted: boolean
}>()
const emit = defineEmits<{
  edit: []
  remove: []
  color: [color: ProjectColor]
  'handle-pointerdown': [event: PointerEvent]
  'handle-keydown': [event: KeyboardEvent]
  'handle-blur': []
}>()

const { t } = useI18n()
const paletteOpen = ref(false)

function pick(color: ProjectColor) {
  paletteOpen.value = false
  emit('color', color)
}
</script>

<template>
  <div
    :data-drag-index="index"
    class="relative flex h-11 items-center gap-1.5 border-b border-border/50 bg-card pl-1.5 pr-2 last:border-b-0"
    :class="(dragging || lifted) && 'rounded-md border border-primary bg-card shadow-md'"
  >
    <button
      type="button"
      class="flex h-7 w-7 shrink-0 cursor-grab items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing"
      :title="t('sessionFlags.settings.dragTip')"
      :aria-label="t('sessionFlags.settings.dragLabel', { name: flagLabel(flag) })"
      :aria-pressed="lifted"
      @pointerdown="emit('handle-pointerdown', $event)"
      @keydown="emit('handle-keydown', $event)"
      @blur="emit('handle-blur')"
    >
      <GripVertical class="h-4 w-4" :stroke-width="1.75" />
    </button>

    <button
      type="button"
      class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring cursor-pointer"
      :title="t(`projectDetail.colors.${flag.color}`)"
      :aria-label="t('sessionFlags.color.title', { name: flagLabel(flag) })"
      :aria-expanded="paletteOpen"
      @click="paletteOpen = !paletteOpen"
    >
      <span class="h-3.5 w-3.5 rounded-full" :class="swatchClass(flag.color)" />
    </button>

    <span class="min-w-0 flex-1 truncate text-sm text-foreground" :title="flagLabel(flag)">{{ flagLabel(flag) }}</span>
    <span class="shrink-0 text-xs tabular-nums text-muted-foreground">
      {{ flag.run_count > 0 ? t('sessionFlags.settings.runs', { n: flag.run_count }) : t('sessionFlags.settings.unused') }}
    </span>

    <Button variant="ghost" size="icon-sm" :title="t('common.edit')" :aria-label="t('common.edit')" @click="emit('edit')">
      <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
    <Button variant="destructive-ghost" size="icon-sm" :title="t('common.delete')" :aria-label="t('common.delete')" @click="emit('remove')">
      <Trash2 class="h-3.5 w-3.5" :stroke-width="2" />
    </Button>

    <FlagColorPopover v-if="paletteOpen" :flag="flag" :flags="flags" @select="pick" @close="paletteOpen = false" />
  </div>
</template>
