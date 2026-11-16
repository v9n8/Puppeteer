<script setup lang="ts">
// A row of a DropdownMenu that opens a menu of its own beside it. Hover or → /
// Enter opens it; ← or Esc closes it again (Esc stops here, so the parent menu
// stays open); a click on a row inside closes the whole menu, as in any
// DropdownMenu. The panel sits to the right of the row and flips to the left
// when the right edge of the window would cut it off. ↑ ↓ inside it are handled
// by the parent DropdownMenu (it knows `data-submenu-panel`).
//
//   <DropdownSubmenu>
//     <template #label>Flag</template>      icon + name
//     <template #hint>In progress</template>  optional, right-aligned value
//     …DropdownItems…
//   </DropdownSubmenu>
import { nextTick, ref } from 'vue'
import { ChevronRight } from 'lucide-vue-next'
import DropdownItem from './DropdownItem.vue'

const open = ref(false)
const flip = ref(false)
const shiftY = ref(0)
const root = ref<HTMLElement | null>(null)
const panel = ref<HTMLElement | null>(null)

const EDGE_MARGIN = 8

function triggerButton(): HTMLElement | null {
  return root.value?.querySelector<HTMLElement>(':scope > button') ?? null
}

async function show(focusFirst = false) {
  open.value = true
  flip.value = false
  shiftY.value = 0
  await nextTick()
  const el = panel.value
  if (!el) return
  const box = el.getBoundingClientRect()
  flip.value = box.right > window.innerWidth - EDGE_MARGIN
  shiftY.value = Math.min(0, window.innerHeight - EDGE_MARGIN - box.bottom)
  if (focusFirst) el.querySelector<HTMLElement>('button:not(:disabled)')?.focus()
}

function hide(refocus = false) {
  open.value = false
  if (refocus) triggerButton()?.focus()
}

function onKeydown(e: KeyboardEvent) {
  const inPanel = !!panel.value?.contains(e.target as Node)
  if (e.key === 'ArrowRight' && !inPanel) {
    e.preventDefault()
    show(true)
  } else if (open.value && (e.key === 'ArrowLeft' || e.key === 'Escape')) {
    e.preventDefault()
    e.stopPropagation()
    hide(true)
  }
}

/** The row opens the submenu; a click must not close the parent menu. */
function onTriggerClick() {
  if (!open.value) show(true)
}
</script>

<template>
  <div ref="root" class="relative" @mouseenter="show()" @mouseleave="hide()" @keydown="onKeydown">
    <DropdownItem aria-haspopup="menu" :aria-expanded="open" @click.stop="onTriggerClick">
      <span class="flex min-w-0 flex-1 items-center gap-2"><slot name="label" /></span>
      <span class="ml-2 max-w-[8rem] truncate text-xs text-muted-foreground"><slot name="hint" /></span>
      <ChevronRight class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
    </DropdownItem>
    <div
      v-if="open"
      ref="panel"
      data-submenu-panel
      role="menu"
      class="ui-popover absolute z-10 min-w-44 rounded-md border border-popover-border bg-popover p-1 shadow-lg shadow-black/20"
      :class="flip ? 'right-full -mr-1' : 'left-full -ml-1'"
      :style="{ top: `${shiftY}px` }"
    >
      <slot />
    </div>
  </div>
</template>
