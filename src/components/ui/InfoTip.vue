<script setup lang="ts">
// ⓘ beside a label, holding the explanation that used to sit under the field
// as a paragraph (UX-33). The browser's `title` shows late, can't wrap to a
// readable width and never opens from the keyboard, so this one:
//  - opens at once on hover or keyboard focus, and closes when they leave;
//  - stays open (pinned) after a click, until Esc, a click outside or another click;
//  - is teleported to <body> and placed by lib/tooltipPosition, so it is never
//    clipped by a scroll container and never runs off the window.
import { nextTick, onBeforeUnmount, ref, useId } from 'vue'
import { useI18n } from 'vue-i18n'
import { Info } from 'lucide-vue-next'
import { placeTooltip } from '@/lib/tooltipPosition'

defineProps<{ text: string }>()

const { t } = useI18n()
const id = useId()

const trigger = ref<HTMLButtonElement | null>(null)
const tip = ref<HTMLElement | null>(null)
const open = ref(false)
const pinned = ref(false)
/** Null until measured: the tip renders hidden for one frame to read its size. */
const pos = ref<{ left: number; top: number } | null>(null)

function place() {
  const a = trigger.value?.getBoundingClientRect()
  const el = tip.value
  if (!a || !el) return
  const { width, height } = el.getBoundingClientRect()
  const p = placeTooltip(a, { width, height }, { width: window.innerWidth, height: window.innerHeight })
  pos.value = { left: p.left, top: p.top }
}

async function show() {
  if (open.value) return
  open.value = true
  pos.value = null
  bind(true)
  await nextTick()
  place()
}

function hide() {
  if (!open.value) return
  open.value = false
  pinned.value = false
  bind(false)
}

function onLeave() {
  if (!pinned.value) hide()
}

function onClick() {
  if (pinned.value) {
    hide()
    return
  }
  pinned.value = true
  show()
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') hide()
}

// Capture phase, so a click on another control closes the tip before that
// control reacts. The trigger itself is ignored: its own click toggles the pin.
function onPointerDown(e: PointerEvent) {
  if (trigger.value && e.target instanceof Node && trigger.value.contains(e.target)) return
  hide()
}

function bind(on: boolean) {
  const fn = on ? window.addEventListener : window.removeEventListener
  fn('keydown', onKeydown as EventListener, true)
  fn('pointerdown', onPointerDown as EventListener, true)
  fn('resize', place)
  fn('scroll', place, true)
}

onBeforeUnmount(() => bind(false))
</script>

<template>
  <button
    ref="trigger"
    type="button"
    class="inline-flex h-4 w-4 shrink-0 cursor-help items-center justify-center rounded-full transition-colors hover:text-foreground focus-visible:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
    :class="pinned ? 'text-foreground' : 'text-muted-foreground'"
    :aria-label="t('common.moreInfo')"
    :aria-describedby="open ? id : undefined"
    @mouseenter="show"
    @mouseleave="onLeave"
    @focus="show"
    @blur="onLeave"
    @click="onClick"
  >
    <Info class="h-3.5 w-3.5" :stroke-width="1.75" />
  </button>
  <Teleport to="body">
    <div
      v-if="open"
      :id="id"
      ref="tip"
      role="tooltip"
      class="ui-popover pointer-events-none fixed z-[100] w-max max-w-[min(20rem,calc(100vw-1rem))] whitespace-pre-line rounded-md border border-popover-border bg-popover px-2.5 py-1.5 text-xs leading-relaxed text-popover-foreground shadow-lg shadow-black/20"
      :style="pos ? { left: `${pos.left}px`, top: `${pos.top}px` } : { left: '0px', top: '0px', visibility: 'hidden' }"
    >
      {{ text }}
    </div>
  </Teleport>
</template>
