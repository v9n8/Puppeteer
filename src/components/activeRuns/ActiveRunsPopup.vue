<script setup lang="ts">
// The Active runs list as a popup, opened from the icon rail's Run icon or the
// compact dock line (the trigger is the default slot). Rows behave like the
// dock's; ↑ ↓ move between rows and Enter opens one. Esc or a click outside
// closes it and focus returns to the trigger (radix restores it).
import { nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { PopoverClose, PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'radix-vue'
import { Cog, X } from 'lucide-vue-next'
import ActiveRunRow from '@/components/activeRuns/ActiveRunRow.vue'
import { useActiveRunRows, type DockRow } from '@/composables/useActiveRunRows'
import { useEscapeClose } from '@/composables/useEscapeClose'
import { useBackgroundProcessesStore } from '@/stores/backgroundProcesses'

withDefaults(defineProps<{ side?: 'top' | 'right' }>(), { side: 'right' })

const { t } = useI18n()
const router = useRouter()
const procs = useBackgroundProcessesStore()
const { displayRows, waitingCount, activeRunId, isFailure, failureLabel, label, projectName, open: openRun } =
  useActiveRunRows()

const isOpen = ref(false)
useEscapeClose(isOpen)
const list = ref<HTMLElement | null>(null)

function items(): HTMLElement[] {
  return Array.from(list.value?.querySelectorAll<HTMLElement>('[data-run-item]') ?? [])
}

function focusFirst(e: Event) {
  // Land on the first row, not the × button radix would pick.
  e.preventDefault()
  nextTick(() => items()[0]?.focus())
}

function onKeydown(e: KeyboardEvent) {
  if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
  const all = items()
  if (all.length === 0) return
  e.preventDefault()
  const at = all.indexOf(document.activeElement as HTMLElement)
  const next = e.key === 'ArrowDown' ? (at + 1) % all.length : (at <= 0 ? all.length - 1 : at - 1)
  all[next].focus()
}

function pick(row: DockRow) {
  isOpen.value = false
  openRun(row)
}

function openProcesses() {
  isOpen.value = false
  router.push('/running?tab=processes').catch(() => {})
}

function seeAll() {
  isOpen.value = false
  router.push('/running').catch(() => {})
}
</script>

<template>
  <PopoverRoot v-model:open="isOpen">
    <PopoverTrigger as-child>
      <slot />
    </PopoverTrigger>
    <PopoverPortal>
      <PopoverContent
        :side="side"
        :align="side === 'right' ? 'end' : 'start'"
        :side-offset="8"
        class="z-[100] w-[340px] rounded-[10px] border border-popover-border bg-popover p-2 text-popover-foreground outline-none"
        @open-auto-focus="focusFirst"
        @keydown="onKeydown"
      >
        <div class="flex items-start gap-2 px-2 pb-2">
          <div class="min-w-0 flex-1">
            <p class="text-sm font-semibold leading-tight">{{ t('misc.activeRuns.heading') }}</p>
            <p class="text-2xs text-muted-foreground leading-tight">
              {{ waitingCount > 0
                ? t('misc.activeRuns.popupSummary', { n: displayRows.length, w: waitingCount })
                : displayRows.length }}
            </p>
          </div>
          <PopoverClose
            class="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :title="t('misc.activeRuns.closeEsc')"
            :aria-label="t('misc.activeRuns.closeEsc')"
          >
            <X class="h-3.5 w-3.5" :stroke-width="2" />
          </PopoverClose>
        </div>

        <div ref="list" class="space-y-0.5">
          <div class="max-h-[260px] space-y-0.5 overflow-y-auto">
            <ActiveRunRow
              v-for="item in displayRows"
              :key="item.row.runId"
              :item="item"
              :active="item.row.runId === activeRunId"
              :label="label(item.row)"
              :project="projectName(item.row.projectId)"
              :failure-text="failureLabel(item.row)"
              :failed="isFailure(item.row.status)"
              @open="pick(item.row)"
            />
          </div>
          <button
            v-if="procs.stray > 0"
            type="button"
            data-run-item
            class="flex w-full cursor-pointer select-none items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs text-warning transition-colors hover:bg-warning/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            @click="openProcesses"
          >
            <Cog class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
            <span class="flex-1">{{ t('misc.activeRuns.strayProcesses', { n: procs.stray }, procs.stray) }}</span>
          </button>
          <button
            type="button"
            data-run-item
            class="w-full cursor-pointer select-none rounded-md px-2 py-1.5 text-left text-xs text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            @click="seeAll"
          >
            {{ t('misc.activeRuns.seeAll') }} →
          </button>
        </div>
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>
