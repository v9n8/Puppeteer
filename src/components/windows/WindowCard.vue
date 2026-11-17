<script setup lang="ts">
// One private window in the "Open windows" overlay: a thumbnail of the window (macOS) or
// else a big icon of its kind, its name, a second line and, for a session, what it
// is doing. The vault's card is the lock icon and nothing else (see cardsFrom).
import { computed, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  AppWindow, ArrowLeftRight, CalendarDays, FileText, GanttChartSquare, GitPullRequest, ListTodo,
  LockKeyhole, MessageSquare, Monitor, Settings, Slack, StickyNote, Target, X,
} from 'lucide-vue-next'
import type { Thumb } from '@/lib/windowThumbs'
import type { WindowCard } from '@/lib/windowsOverlay'

const props = defineProps<{
  card: WindowCard
  thumb: Thumb
  selected: boolean
  /** The close button's tooltip, with the key. */
  closeTip: string
}>()
const emit = defineEmits<{
  activate: []
  close: []
  select: []
  menu: [event: MouseEvent]
}>()

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
    class="group relative flex w-full cursor-pointer flex-col gap-1 rounded-[10px] border border-border bg-card p-2 text-left transition-colors hover:bg-accent"
    :class="selected ? 'ring-2 ring-ring' : ''"
    @click="emit('activate')"
    @contextmenu.prevent="emit('select'); emit('menu', $event)"
  >
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
        v-if="card.isHere"
        class="absolute left-1.5 top-1.5 inline-flex h-[22px] items-center rounded-full bg-primary/12 px-2 text-[13px] font-medium text-primary"
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
      <span class="truncate text-[13px] font-medium text-foreground" :title="card.title">{{ card.title }}</span>
    </div>
    <span v-if="card.sub" class="truncate text-[13px] text-muted-foreground" :title="card.sub">{{ card.sub }}</span>
  </div>
</template>
