<script setup lang="ts">
// One column of the board: head (dot, name, count), the cards (scrolling on their own)
// and the drop state while a card is dragged. "No flag" can fold into a 44px strip;
// the All projects board's "Own flags" column carries a lock and takes no drop.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronsLeft, ChevronsRight, CircleDashed, Lock } from 'lucide-vue-next'
import { swatchClass } from '@/lib/projectAppearance'
import type { BoardColumn } from '@/lib/flagBoard'
import type { BoardRun } from '@/stores/flagBoard'

const props = defineProps<{
  column: BoardColumn
  label: string
  cards: BoardRun[]
  matched: number
  total: number
  /** The filter box has text. */
  filtering: boolean
  query: string
  collapsed: boolean
  /** While a card is dragged over: where it would land, can't land, or this column is not the one. */
  drop: 'idle' | 'target' | 'blocked'
  /** Why a blocked drop is refused (a Project hides the flag). */
  blockedReason: string
}>()

const emit = defineEmits<{ toggle: [] }>()
const { t } = useI18n()

const count = computed(() => (props.filtering ? `${props.matched} / ${props.total}` : String(props.total)))
const emptyText = computed(() => {
  if (props.filtering && props.total > 0) return t('board.column.noMatch', { query: props.query.trim() })
  if (props.column.kind === 'none') return t('board.column.emptyNone')
  if (props.column.kind === 'own') return t('board.column.emptyOwn')
  return t('board.column.emptyFlag', { flag: props.label })
})
const ownLock = computed(() => t('board.column.ownLock'))
</script>

<template>
  <section
    :data-board-column="column.id"
    class="relative flex min-h-0 flex-col rounded-card border bg-muted/30"
    :class="[
      collapsed ? 'w-11 shrink-0' : 'min-w-[12.25rem] max-w-[20rem] flex-1 basis-[12.25rem]',
      drop === 'target' ? 'border-dashed border-primary bg-accent' : drop === 'blocked' ? 'cursor-not-allowed border-dashed border-destructive/60' : 'border-border',
    ]"
    :aria-label="`${label} · ${count}`"
    :title="drop === 'blocked' ? blockedReason : column.kind === 'own' ? ownLock : undefined"
  >
    <!-- Folded "No flag": a strip with the name written downwards. -->
    <button
      v-if="collapsed"
      type="button"
      class="flex h-full min-h-0 w-full cursor-pointer flex-col items-center gap-2 rounded-card py-2 text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :title="t('board.column.unfold')"
      :aria-label="t('board.column.unfold')"
      @click="emit('toggle')"
    >
      <ChevronsRight class="h-4 w-4 shrink-0" :stroke-width="1.75" />
      <span class="font-mono text-2xs">{{ count }}</span>
      <span class="text-2xs font-medium" style="writing-mode: vertical-rl">{{ label }}</span>
    </button>

    <template v-else>
      <header class="flex h-9 shrink-0 items-center gap-2 px-2.5">
        <CircleDashed v-if="column.kind === 'none'" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
        <Lock v-else-if="column.kind === 'own'" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
        <span v-else-if="column.flag" class="h-2.5 w-2.5 shrink-0 rounded-full" :class="swatchClass(column.flag.color)" aria-hidden="true" />
        <h2 class="min-w-0 flex-1 truncate text-sm font-medium" :title="label">{{ label }}</h2>
        <span class="shrink-0 font-mono text-2xs text-muted-foreground" :title="t('board.column.cardCount', { n: total })">{{ count }}</span>
        <button
          v-if="column.kind === 'none'"
          type="button"
          class="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          :title="t('board.column.fold')"
          :aria-label="t('board.column.fold')"
          @click="emit('toggle')"
        >
          <ChevronsLeft class="h-4 w-4" :stroke-width="1.75" />
        </button>
      </header>

      <div class="min-h-0 flex-1 space-y-2 overflow-y-auto px-2 pb-2">
        <p v-if="drop === 'blocked'" class="px-1 py-2 text-center text-2xs text-destructive">{{ blockedReason }}</p>
        <template v-for="card in cards" :key="card.id">
          <slot name="card" :card="card" />
        </template>
        <p v-if="!cards.length && drop !== 'blocked'" class="px-1 py-6 text-center text-2xs text-muted-foreground">{{ emptyText }}</p>
      </div>
      <!-- Where a dropped card will land: the end of the column. -->
      <div v-if="drop === 'target'" class="pointer-events-none absolute inset-x-2 bottom-1.5 h-0.5 rounded bg-primary" />
    </template>
  </section>
</template>
