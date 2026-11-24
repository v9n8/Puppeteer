<script setup lang="ts">
// One of the project's own flags in Project › Flag (V3-21): drag handle, colour dot,
// name, how many Sessions carry it, Edit, Delete. Like Settings › Flag's row, but the
// dot only shows the colour (the editor changes it). Drag and keyboard moves are the
// list's job: the row only reports the handle's events.
import { useI18n } from 'vue-i18n'
import { GripVertical, Pencil, Trash2 } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { swatchClass } from '@/lib/projectAppearance'
import { flagLabel, type SessionFlag } from '@/lib/sessionFlags'

defineProps<{
  flag: SessionFlag
  index: number
  /** Being dragged with the pointer. */
  dragging: boolean
  /** Lifted with Space: arrows move it. */
  lifted: boolean
}>()
const emit = defineEmits<{
  edit: []
  remove: []
  'handle-pointerdown': [event: PointerEvent]
  'handle-keydown': [event: KeyboardEvent]
  'handle-blur': []
}>()

const { t } = useI18n()
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
      :title="t('sessionFlags.project.dragTip')"
      :aria-label="t('sessionFlags.project.dragLabel', { name: flagLabel(flag) })"
      :aria-pressed="lifted"
      @pointerdown="emit('handle-pointerdown', $event)"
      @keydown="emit('handle-keydown', $event)"
      @blur="emit('handle-blur')"
    >
      <GripVertical class="h-4 w-4" :stroke-width="1.75" />
    </button>

    <span class="h-3 w-3 shrink-0 rounded-full" :class="swatchClass(flag.color)" />
    <span class="min-w-0 flex-1 truncate text-sm text-foreground" :title="flagLabel(flag)">{{ flagLabel(flag) }}</span>
    <span class="shrink-0 text-xs tabular-nums text-muted-foreground">
      {{ flag.run_count > 0 ? t('sessionFlags.project.runs', { n: flag.run_count }) : t('sessionFlags.project.unused') }}
    </span>

    <Button variant="ghost" size="icon-sm" :title="t('common.edit')" :aria-label="t('common.edit')" @click="emit('edit')">
      <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
    <Button variant="destructive-ghost" size="icon-sm" :title="t('common.delete')" :aria-label="t('common.delete')" @click="emit('remove')">
      <Trash2 class="h-3.5 w-3.5" :stroke-width="2" />
    </Button>
  </div>
</template>
