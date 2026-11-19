<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { computeDiff } from '@/lib/diff'
import { splitDiffRows } from '@/lib/diffSplit'

const { t } = useI18n()

const props = defineProps<{
  before: string
  after: string
  /** Unchanged-line padding kept around each change before folding. */
  context?: number
  /** Two columns (before | after) instead of one unified list. */
  split?: boolean
}>()

const result = computed(() => computeDiff(props.before, props.after, { context: props.context ?? 3 }))
const splitRows = computed(() => (props.split ? splitDiffRows(result.value.rows) : []))
</script>

<template>
  <!-- Two columns: removed lines on the left, added on the right -->
  <div
    v-if="split"
    class="rounded-md border border-border overflow-hidden font-mono text-2xs leading-relaxed max-h-[28rem] overflow-y-auto"
  >
    <div v-for="(row, i) in splitRows" :key="i" class="flex">
      <span
        v-if="row.type === 'fold'"
        class="w-full bg-foreground/2 px-3 py-0.5 text-center text-2xs text-foreground/35 select-none"
      >{{ t('misc.diff.unchangedLines', { count: row.count }) }}</span>
      <template v-else>
        <div
          v-for="(cell, side) in [row.left, row.right]"
          :key="side"
          class="flex min-w-0 flex-1"
          :class="[
            side === 0 ? 'border-r border-border' : '',
            cell?.type === 'add' ? 'bg-emerald-500/10 dark:bg-emerald-500/8' : '',
            cell?.type === 'del' ? 'bg-red-500/10 dark:bg-red-500/8' : '',
          ]"
        >
          <span class="w-9 shrink-0 px-1 text-right tabular-nums text-foreground/30 select-none">{{ cell?.no ?? '' }}</span>
          <span
            class="min-w-0 flex-1 px-2 whitespace-pre-wrap break-words"
            :class="{
              'text-emerald-700 dark:text-emerald-100/90': cell?.type === 'add',
              'text-red-700 dark:text-red-200/90': cell?.type === 'del',
              'text-foreground/60': cell?.type === 'ctx',
            }"
          >{{ cell ? (cell.text || ' ') : ' ' }}</span>
        </div>
      </template>
    </div>
  </div>
  <div v-else class="rounded-md border border-border overflow-hidden font-mono text-2xs leading-relaxed max-h-[28rem] overflow-y-auto">
    <div
      v-for="(row, i) in result.rows"
      :key="i"
      class="flex"
      :class="{
        'bg-emerald-500/10 dark:bg-emerald-500/8': row.type === 'add',
        'bg-red-500/10 dark:bg-red-500/8': row.type === 'del',
        'bg-foreground/2': row.type === 'fold',
      }"
    >
      <!-- Fold marker for collapsed unchanged runs -->
      <template v-if="row.type === 'fold'">
        <span class="w-full px-3 py-0.5 text-center text-2xs text-foreground/35 select-none">
          {{ t('misc.diff.unchangedLines', { count: row.count }) }}
        </span>
      </template>
      <template v-else>
        <!-- Old / new line-number gutters -->
        <span class="w-9 shrink-0 px-1 text-right tabular-nums text-foreground/30 select-none">{{ row.oldNo ?? '' }}</span>
        <span class="w-9 shrink-0 px-1 text-right tabular-nums text-foreground/30 select-none border-r border-border">{{ row.newNo ?? '' }}</span>
        <!-- +/- marker -->
        <span
          class="w-4 shrink-0 text-center select-none"
          :class="{
            'text-emerald-600 dark:text-emerald-400/80': row.type === 'add',
            'text-red-600 dark:text-red-400/80': row.type === 'del',
            'text-foreground/20': row.type === 'ctx',
          }"
        >{{ row.type === 'add' ? '+' : row.type === 'del' ? '−' : '' }}</span>
        <!-- Line text -->
        <span
          class="min-w-0 flex-1 px-2 whitespace-pre-wrap break-words [overflow-wrap:anywhere]"
          :class="{
            'text-emerald-700 dark:text-emerald-100/90': row.type === 'add',
            'text-red-700 dark:text-red-200/90': row.type === 'del',
            'text-foreground/60': row.type === 'ctx',
          }"
        >{{ row.text || ' ' }}</span>
      </template>
    </div>
  </div>
</template>
