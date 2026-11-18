<script setup lang="ts">
// One Session on the board: status dot, name (two lines at most), when it was last
// worked on and how many workers it has. Pressing it opens the Session; dragging it
// to another column changes its flag (BoardView owns both).
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Clock, Loader2, Network, Pin } from 'lucide-vue-next'
import { relativeAge } from '@/lib/runDisplay'
import { runActivityAt } from '@/stores/runs'
import { runStatusKey } from '@/lib/interruptedRuns'
import { cardStatus, cardTitle, type CardStatus } from '@/lib/flagBoard'
import { swatchClass } from '@/lib/projectAppearance'
import type { BoardRun } from '@/stores/flagBoard'
import type { SessionFlag } from '@/lib/sessionFlags'

const props = defineProps<{
  run: BoardRun
  /** The Session waits for the user's permission (from the live store). */
  waiting: boolean
  /** Spinner: its flag is being saved. */
  saving: boolean
  /** Show the Project's name (All projects board). */
  showProject: boolean
  /** The Session's own flag, shown as a chip on the "Own flags" column. */
  ownFlag: SessionFlag | null
  /** Lifted with the keyboard, or being dragged: drawn raised / faded. */
  state: 'idle' | 'lifted' | 'dragging'
}>()

const emit = defineEmits<{
  open: []
  menu: [x: number, y: number]
  press: [event: PointerEvent]
}>()

const { t } = useI18n()
const title = computed(() => cardTitle(props.run, t))
const status = computed<CardStatus>(() => cardStatus(props.run, props.waiting))
const statusTitle = computed(() =>
  status.value === 'waiting' ? t('board.card.waiting') : status.value === 'running' ? t('common.status.running') : t(runStatusKey(props.run)),
)
const DOT: Record<CardStatus, string> = {
  running: 'bg-info',
  waiting: 'bg-warning',
  done: 'bg-success',
  failed: 'bg-destructive',
  cancelled: 'bg-warning/60',
  draft: 'bg-muted-foreground/50',
}
const age = computed(() => relativeAge(runActivityAt(props.run), t('run.relNow')))
const flagName = computed(() => (props.ownFlag ? props.ownFlag.name : ''))

function onContextMenu(ev: MouseEvent) {
  ev.preventDefault()
  emit('menu', ev.clientX, ev.clientY)
}

// Shift+F10 / the Menu key opens the same menu beside the card.
function onKeydown(ev: KeyboardEvent) {
  if (ev.key !== 'ContextMenu' && !(ev.shiftKey && ev.key === 'F10')) return
  ev.preventDefault()
  ev.stopPropagation()
  const rect = (ev.currentTarget as HTMLElement).getBoundingClientRect()
  emit('menu', rect.left + 24, rect.bottom - 8)
}
</script>

<template>
  <button
    type="button"
    :data-card-id="run.id"
    class="flex min-h-[2.75rem] w-full cursor-pointer flex-col gap-1.5 rounded-md border bg-card px-2.5 py-2 text-left transition-colors hover:bg-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :class="[
      state === 'lifted' ? 'border-primary shadow-md ring-2 ring-primary/40' : 'border-border',
      state === 'dragging' ? 'opacity-40' : '',
    ]"
    :title="t('board.card.open')"
    :aria-label="`${title} · ${statusTitle}`"
    @click="emit('open')"
    @pointerdown="emit('press', $event)"
    @contextmenu="onContextMenu"
    @keydown="onKeydown"
  >
    <span class="flex items-start gap-2">
      <Loader2 v-if="saving" class="mt-0.5 h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground" :stroke-width="2" :title="t('board.card.busy')" />
      <span
        v-else
        class="mt-1.5 h-2 w-2 shrink-0 rounded-full"
        :class="[DOT[status], { 'animate-pulse': status === 'running' || status === 'waiting' }]"
        :title="statusTitle"
        role="img"
        :aria-label="statusTitle"
      />
      <Pin v-if="run.pinned" class="mt-0.5 h-3 w-3 shrink-0 text-primary" :stroke-width="2" />
      <span class="line-clamp-2 min-w-0 flex-1 break-words text-sm font-medium leading-5">{{ title }}</span>
    </span>
    <span v-if="showProject" class="truncate pl-4 text-2xs text-muted-foreground">{{ run.project_name }}</span>
    <span v-if="ownFlag" class="flex items-center gap-1.5 pl-4 text-2xs text-muted-foreground" :title="t('board.card.ownFlag', { name: flagName })">
      <span class="h-2 w-2 shrink-0 rounded-full" :class="swatchClass(ownFlag.color)" aria-hidden="true" />
      <span class="truncate">{{ flagName }}</span>
    </span>
    <span class="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-2xs leading-[1.4] text-muted-foreground">
      <span class="flex items-center gap-1" :title="t('board.card.updated', { age })">
        <Clock class="h-3 w-3" :stroke-width="1.75" />{{ age }}
      </span>
      <span v-if="run.worker_count > 0" class="flex items-center gap-1" :title="t('board.card.workers', { n: run.worker_count })">
        <Network class="h-3 w-3" :stroke-width="1.75" />{{ t('board.card.workers', { n: run.worker_count }) }}
      </span>
    </span>
  </button>
</template>
