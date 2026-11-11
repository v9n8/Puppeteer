<script setup lang="ts">
// The row of buttons under one chat turn — under the question bubble, and after
// the last entry of the answer (components/StreamLog.vue places it). One
// component for both sides so they stay alike; which buttons show is decided by
// lib/turnActions. Frequent actions sit inline, rare ones behind `⋯`, which
// opens upward first because the row ends its turn.
import { computed, onBeforeUnmount, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  Bookmark, BookmarkCheck, Check, Copy, GitFork, History, Layers, Maximize2, MoreHorizontal, Pencil, Quote, StickyNote,
} from 'lucide-vue-next'
import { Button, DropdownItem, DropdownMenu, DropdownSeparator } from '@/components/ui'
import { turnActions, type TurnAction, type TurnSide } from '@/lib/turnActions'

/**
 * State every row of one log reads. Passed as one object of stable identity:
 * StreamLog freezes finished entries with v-memo, so a row has to read what can
 * change (a quote target appearing, a feature switched off) itself.
 */
export interface MsgActionsState {
  isOn: (featureId: string) => boolean
  /** Name of the session a quote goes to; null = Quote hidden. */
  quoteLabel: Readonly<Ref<string | null>>
  canSaveNote: Readonly<Ref<boolean>>
  /** The session can be rewound to `turn` now (reads its state reactively). */
  canRewind: (turn: number) => boolean
  /** The session can be forked now (reads its state reactively). */
  canFork: () => boolean
  /** The message sent as `turn` can be edited and resent now (reads its state reactively). */
  canEditResend: (turn: number) => boolean
  /**
   * Whether one side of the turn starting at entry `promptAt` is bookmarked;
   * null = it cannot be bookmarked here (reads its state reactively).
   */
  bookmarked: (promptAt: number, side: TurnSide) => boolean | null
}

const props = defineProps<{
  side: TurnSide
  hasText: boolean
  /** The snapshot turn "Rewind to here" goes back to; null = none. */
  rewindTurn: number | null
  /** Entry index of the prompt this row's turn starts with; -1 = before the loaded window. */
  promptAt: number
  state: MsgActionsState
  /**
   * Runs an action clicked on the button at `from` (Quote opens its popover
   * there); resolves true when Copy actually wrote the clipboard.
   */
  run: (action: TurnAction, from: DOMRect) => Promise<boolean>
}>()

const { t } = useI18n()

const bookmarked = computed(() => props.state.bookmarked(props.promptAt, props.side))

const buttons = computed(() =>
  turnActions(props.side, props.state.isOn, {
    hasText: props.hasText,
    hasQuoteTarget: props.state.quoteLabel.value !== null,
    canSaveNote: props.state.canSaveNote.value,
    canRewind: props.rewindTurn !== null && props.state.canRewind(props.rewindTurn),
    canFork: props.state.canFork(),
    canEditResend: props.side === 'question' && props.rewindTurn !== null && props.state.canEditResend(props.rewindTurn),
    bookmarked: bookmarked.value,
  }),
)

// The tick replaces the copy icon only once the clipboard write succeeded.
const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | undefined
async function click(action: TurnAction, e: MouseEvent) {
  const ok = await props.run(action, (e.currentTarget as HTMLElement).getBoundingClientRect())
  if (action !== 'copy' || !ok) return
  copied.value = true
  clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => { copied.value = false }, 1500)
}
onBeforeUnmount(() => clearTimeout(copiedTimer))

const ICONS = {
  copy: Copy, quote: Quote, bookmark: Bookmark, fullscreen: Maximize2, fullscreenTurn: Layers, saveNote: StickyNote, fork: GitFork,
  editResend: Pencil, rewind: History,
} as const

function icon(action: TurnAction) {
  if (action === 'bookmark' && bookmarked.value) return BookmarkCheck
  return action === 'copy' && copied.value ? Check : ICONS[action]
}

function title(action: TurnAction): string {
  if (action === 'copy' && copied.value) return t('streamLog.turn.copied')
  if (action === 'bookmark' && bookmarked.value) return t('streamLog.turn.unbookmark')
  if (action === 'quote') return t('streamLog.turn.quote', { target: props.state.quoteLabel.value ?? '' })
  return t(`streamLog.turn.${action}`)
}
</script>

<template>
  <div v-if="buttons.primary.length || buttons.overflow.length" class="flex items-center gap-0.5">
    <Button
      v-for="action in buttons.primary"
      :key="action"
      variant="ghost"
      size="icon-xs"
      class="text-foreground/40"
      :class="{ 'text-primary': (action === 'copy' && copied) || (action === 'bookmark' && bookmarked) }"
      :title="title(action)"
      :aria-label="title(action)"
      @click="click(action, $event)"
    >
      <component :is="icon(action)" class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
    <DropdownMenu v-if="buttons.overflow.length" align="left" prefer="above">
      <template #trigger>
        <Button
          variant="ghost"
          size="icon-xs"
          class="text-foreground/40"
          :title="t('streamLog.turn.more')"
          :aria-label="t('streamLog.turn.more')"
        >
          <MoreHorizontal class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
      </template>
      <template v-for="(action, n) in buttons.overflow" :key="action">
        <!-- Rewind cuts the session: kept apart from the harmless items. -->
        <DropdownSeparator v-if="action === 'rewind' && n > 0" />
        <DropdownItem :variant="action === 'rewind' ? 'destructive' : 'default'" @click="click(action, $event)">
          <component :is="icon(action)" class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ title(action) }}
        </DropdownItem>
      </template>
    </DropdownMenu>
  </div>
</template>
