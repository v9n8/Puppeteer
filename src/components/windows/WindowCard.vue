<script setup lang="ts">
// One private window in the "Open windows" overlay: a thumbnail of the window (macOS) or
// else a big icon of its kind, its name, a second line and, for a session, what it
// is doing. The vault's card is the lock icon and nothing else (see cardsFrom).
// A round checkbox marks it for the actions on several windows (WindowsSelectionBar);
// dragged onto the drop strip it moves back to the main window (useWindowsDrag).
import { computed, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  AppWindow, ArrowLeftRight, CalendarDays, Check, FileText, GanttChartSquare, GitPullRequest, ListTodo,
  LockKeyhole, MessageSquare, Monitor, Settings, Slack, SquareArrowDownLeft, StickyNote, Target, X,
} from 'lucide-vue-next'
import type { Thumb } from '@/lib/windowThumbs'
import type { WindowCard } from '@/lib/windowsOverlay'

export interface MarkClick {
  /** ⇧: mark the range from the last marked card. */
  shift: boolean
  /** ⌘ / Ctrl. */
  meta: boolean
}

const props = defineProps<{
  card: WindowCard
  thumb: Thumb
  selected: boolean
  /** The close button's tooltip, with the key. */
  closeTip: string
  marked: boolean
  /** Some card is marked: every checkbox shows, and a plain click marks instead of opening (no window opened by mistake). */
  markMode: boolean
  /** Lifted by a drag (useWindowsDrag): the card stays in place, faded. */
  dragging?: boolean
}>()
const emit = defineEmits<{
  activate: []
  close: []
  select: []
  mark: [click: MarkClick]
  press: [event: PointerEvent]
  menu: [event: MouseEvent]
}>()

function onClick(e: MouseEvent) {
  const meta = e.metaKey || e.ctrlKey
  if (e.shiftKey || meta || props.markMode) emit('mark', { shift: e.shiftKey, meta })
  else emit('activate')
}

const { t } = useI18n()

const SCREEN_ICONS: Record<string, Component> = {
  gantt: GanttChartSquare,
  calendar: CalendarDays,
  'pr-inbox': GitPullRequest,
  focus: Target,
  'slack-threads': Slack,
  'item-list-todo': ListTodo,
  'item-list-note': StickyNote,
  'item-create': StickyNote,
  settings: Settings,
  'file-transfer': ArrowLeftRight,
}

const icon = computed<Component>(() => {
  switch (props.card.kind) {
    case 'session': return MessageSquare
    case 'file': return FileText
    case 'secret': return LockKeyhole
    case 'remote': return Monitor
    default: return SCREEN_ICONS[props.card.label] ?? AppWindow
  }
})

const BADGE_CLASS = {
  running: 'bg-info/15 text-info',
  waiting: 'bg-warning/15 text-warning',
  error: 'bg-destructive/10 text-destructive',
  minimized: 'border border-border bg-card text-muted-foreground',
} as const

// Waiting beats running beats minimized: one badge per card, as in the mockup.
const badge = computed(() => {
  const status = props.card.status
  if (status) return { text: t(`windows.status.${status}`), cls: BADGE_CLASS[status] }
  if (props.card.minimized) return { text: t('windows.status.minimized'), cls: BADGE_CLASS.minimized }
  return null
})
</script>

<template>
  <div
    role="option"
    :aria-selected="selected"
    :data-window-label="card.label"
    class="group relative flex w-full cursor-pointer flex-col rounded-card border bg-card text-left transition-colors hover:bg-accent"
    :class="[marked ? 'border-primary' : 'border-border', selected ? 'ring-2 ring-ring' : '', dragging ? 'opacity-40' : '']"
    @pointerdown="emit('press', $event)"
    @click="onClick"
    @contextmenu.prevent="emit('select'); emit('menu', $event)"
  >
    <div class="flex min-w-0 flex-col gap-1 p-2">
    <div class="relative flex aspect-[16/10] items-center justify-center overflow-hidden rounded-md bg-muted text-muted-foreground">
      <img
        v-if="thumb.kind === 'image'"
        :src="thumb.src"
        alt=""
        class="absolute inset-0 h-full w-full object-cover object-top"
        draggable="false"
      />
      <span v-else-if="thumb.kind === 'loading'" class="absolute inset-0 animate-pulse bg-muted" :aria-label="t('windows.thumb.loading')" />
      <component :is="icon" v-else class="h-8 w-8" :stroke-width="1.25" aria-hidden="true" />
      <span v-if="badge" class="absolute bottom-1.5 left-1.5 rounded-full bg-card">
        <span
          class="inline-flex h-[22px] items-center rounded-full px-2 text-[13px] font-medium"
          :class="badge.cls"
        >{{ badge.text }}</span>
      </span>
      <span
        v-if="!card.canMerge"
        class="absolute bottom-1.5 right-1.5 flex h-[22px] items-center gap-1 rounded-full bg-card px-1.5 text-muted-foreground"
        :title="t('windows.cannotMerge')"
      >
        <SquareArrowDownLeft class="h-3.5 w-3.5" :stroke-width="1.75" aria-hidden="true" />
        <X class="h-3 w-3" :stroke-width="2.5" aria-hidden="true" />
        <span class="sr-only">{{ t('windows.cannotMerge') }}</span>
      </span>
      <button
        type="button"
        role="checkbox"
        :aria-checked="marked"
        :title="t('windows.mark')"
        :aria-label="t('windows.mark')"
        class="absolute left-1.5 top-1.5 z-[1] flex h-5 w-5 cursor-pointer items-center justify-center rounded-full transition-opacity before:absolute before:-inset-1 before:content-[''] focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="marked
          ? 'bg-primary text-primary-foreground ring-2 ring-card'
          : ['border-2 border-card bg-background/80 ring-1 ring-input', markMode ? 'opacity-100' : 'opacity-0 group-hover:opacity-100']"
        @click.stop="emit('mark', { shift: $event.shiftKey, meta: $event.metaKey || $event.ctrlKey })"
      >
        <Check v-if="marked" class="h-3.5 w-3.5" :stroke-width="3" aria-hidden="true" />
      </button>
      <span
        v-if="card.isHere"
        class="absolute left-8 top-1.5 inline-flex h-[22px] items-center rounded-full bg-primary/12 px-2 text-[13px] font-medium text-primary"
      >{{ t('windows.here') }}</span>
      <button
        type="button"
        class="absolute right-1.5 top-1.5 flex h-7 w-7 items-center justify-center rounded-md bg-background/80 text-muted-foreground opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring group-hover:opacity-100"
        :class="selected ? 'opacity-100' : ''"
        :title="closeTip"
        :aria-label="closeTip"
        @click.stop="emit('close')"
      >
        <X class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
      </button>
    </div>
    <div class="flex min-w-0 items-center gap-1.5">
      <component :is="icon" class="h-4 w-4 shrink-0 text-muted-foreground" :stroke-width="1.75" aria-hidden="true" />
      <span class="truncate text-sm font-medium leading-5 text-foreground" :title="card.title">{{ card.title }}</span>
    </div>
    <span v-if="card.sub" class="min-h-[22px] truncate text-sm text-muted-foreground" :title="card.sub">{{ card.sub }}</span>
    </div>
  </div>
</template>
