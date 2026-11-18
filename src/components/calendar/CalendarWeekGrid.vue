<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { CalEvent } from '@/stores/googleCalendar'
import { startOfWeek, addDays, sameDay, minutesOfDay, parseEventTime } from '@/lib/calendar'
import { lunarInfo } from '@/lib/lunar'
import { isOngoing, nowLineTop } from '@/lib/calendarNow'
import { allDayByDay } from '@/lib/calendarAllDay'
import { useMinuteClock } from '@/composables/useMinuteClock'

const props = defineProps<{
  anchorDate: Date
  events: CalEvent[]
  colorFor: (accountId: string) => string
  showLunar?: boolean
}>()

const emit = defineEmits<{
  select: [ev: CalEvent]
  createAt: [date: Date]
}>()

const { t } = useI18n()

const HOUR_HEIGHT = 44 // px per hour

/** Click an empty spot in a day column → create an event at that hour. */
function onColumnClick(day: Date, e: MouseEvent) {
  const col = e.currentTarget as HTMLElement
  const y = e.clientY - col.getBoundingClientRect().top
  const hour = Math.max(0, Math.min(23, Math.floor(y / HOUR_HEIGHT)))
  const start = new Date(day)
  start.setHours(hour, 0, 0, 0)
  emit('createAt', start)
}
const HOURS = Array.from({ length: 24 }, (_, i) => i)
const WEEKDAY_KEYS = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun']
const WEEKDAYS = computed(() => WEEKDAY_KEYS.map(k => t(`calendar.week.${k}`)))

const weekDays = computed(() => {
  const start = startOfWeek(props.anchorDate)
  return Array.from({ length: 7 }, (_, i) => addDays(start, i))
})

// Ticks every minute: moves the now line, flips the "today" column after midnight
// and ends the "ongoing" state of events.
const today = useMinuteClock()

interface Parsed {
  ev: CalEvent
  start: Date
  end: Date
  allDay: boolean
}

const parsed = computed<Parsed[]>(() =>
  props.events.map(ev => ({ ev, ...parseEventTime(ev) })),
)

// ── All-day events: shown in the header strip, on each covered day of the week ──
const allDayCells = computed(() => allDayByDay(parsed.value, weekDays.value))

// ── Timed events: positioned blocks with overlap-aware lane packing ──
interface Block {
  ev: CalEvent
  start: Date
  top: number
  height: number
  leftPct: number
  widthPct: number
}

function timedBlocks(day: Date): Block[] {
  const items = parsed.value
    .filter(p => !p.allDay && sameDay(p.start, day))
    .sort((a, b) => a.start.getTime() - b.start.getTime())
  if (!items.length) return []

  // Split into clusters of transitively-overlapping events.
  const blocks: Block[] = []
  let cluster: Parsed[] = []
  let clusterEnd = 0

  const flush = () => {
    if (!cluster.length) return
    // Assign each event to the first free lane.
    const laneEnds: number[] = []
    const laneOf = new Map<Parsed, number>()
    for (const it of cluster) {
      const s = it.start.getTime()
      let lane = laneEnds.findIndex(end => end <= s)
      if (lane === -1) {
        lane = laneEnds.length
        laneEnds.push(0)
      }
      laneEnds[lane] = it.end.getTime()
      laneOf.set(it, lane)
    }
    const lanes = laneEnds.length
    for (const it of cluster) {
      const startMin = minutesOfDay(it.start)
      const endMin = it.end.getTime() > it.start.getTime()
        ? Math.min(24 * 60, minutesOfDay(it.start) + (it.end.getTime() - it.start.getTime()) / 60000)
        : startMin + 30
      const lane = laneOf.get(it)!
      blocks.push({
        ev: it.ev,
        start: it.start,
        top: (startMin / 60) * HOUR_HEIGHT,
        height: Math.max(14, ((endMin - startMin) / 60) * HOUR_HEIGHT),
        leftPct: (lane / lanes) * 100,
        widthPct: 100 / lanes,
      })
    }
    cluster = []
    clusterEnd = 0
  }

  for (const it of items) {
    if (cluster.length && it.start.getTime() >= clusterEnd) flush()
    cluster.push(it)
    clusterEnd = Math.max(clusterEnd, it.end.getTime())
  }
  flush()
  return blocks
}

function nowLabel() {
  return hhmm(today.value)
}

function hhmm(d: Date) {
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

// Scroll the time grid to ~7am on first render.
const scrollBody = ref<HTMLDivElement | null>(null)
onMounted(() => {
  if (scrollBody.value) scrollBody.value.scrollTop = 7 * HOUR_HEIGHT
})
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Day header + all-day strip -->
    <div class="flex shrink-0 border-b border-border">
      <div class="w-14 shrink-0" />
      <div class="grid grid-cols-7 flex-1">
        <div
          v-for="(d, i) in weekDays"
          :key="i"
          class="flex flex-col items-center gap-0.5 border-l border-border px-1 py-1.5"
        >
          <span class="text-xs leading-none text-muted-foreground">{{ WEEKDAYS[i] }}</span>
          <span
            class="inline-grid h-[26px] min-w-[26px] place-items-center rounded-full text-md font-semibold"
            :class="sameDay(d, today) ? 'bg-primary text-primary-foreground' : ''"
          >{{ d.getDate() }}</span>
          <span
            v-if="showLunar"
            class="text-xs leading-none tabular-nums"
            :class="lunarInfo(d).highlight ? 'font-semibold text-destructive' : 'text-muted-foreground'"
          >{{ lunarInfo(d).label }}</span>
        </div>
      </div>
    </div>
    <!-- All-day row -->
    <div class="flex min-h-[30px] shrink-0 border-b border-border">
      <div class="flex w-14 shrink-0 items-center justify-end whitespace-nowrap pr-1.5 text-xs text-muted-foreground">{{ t('calendar.view.allDay') }}</div>
      <div class="grid grid-cols-7 flex-1">
        <div
          v-for="(cell, i) in allDayCells"
          :key="i"
          class="flex min-w-0 flex-col gap-0.5 border-l border-border px-0.5 py-[3px]"
        >
          <button
            v-for="p in cell"
            :key="p.ev.id + p.ev.calendar_id"
            type="button"
            class="truncate rounded px-1 py-0.5 text-left text-2xs text-white"
            :style="{ backgroundColor: colorFor(p.ev.account_id) }"
            :title="p.ev.title"
            @click="emit('select', p.ev)"
          >{{ p.ev.title }}</button>
        </div>
      </div>
    </div>

    <!-- Scrollable time grid -->
    <div ref="scrollBody" class="flex-1 overflow-y-auto min-h-0">
      <div class="flex" :style="{ height: `${24 * HOUR_HEIGHT}px` }">
        <!-- Hour labels -->
        <div class="relative w-14 shrink-0">
          <!-- "Now" time sits in the gutter, so it never covers an event -->
          <span
            v-if="weekDays.some((d) => sameDay(d, today))"
            class="pointer-events-none absolute right-1.5 z-10 -translate-y-1/2 rounded bg-primary px-1 text-2xs leading-tight text-primary-foreground tabular-nums"
            :style="{ top: `${nowLineTop(today, 0, HOUR_HEIGHT)}px` }"
          >{{ nowLabel() }}</span>
          <div
            v-for="h in HOURS"
            :key="h"
            class="relative border-b border-border/30"
            :style="{ height: `${HOUR_HEIGHT}px` }"
          >
            <span class="absolute -top-1.5 right-1 text-2xs text-muted-foreground">
              {{ h > 0 ? String(h).padStart(2, '0') + ':00' : '' }}
            </span>
          </div>
        </div>

        <!-- Day columns -->
        <div class="grid grid-cols-7 flex-1">
          <div
            v-for="(d, i) in weekDays"
            :key="i"
            class="relative border-l border-border cursor-pointer"
            @click="onColumnClick(d, $event)"
          >
            <!-- hour lines -->
            <div
              v-for="h in HOURS"
              :key="h"
              class="border-b border-border/30"
              :style="{ height: `${HOUR_HEIGHT}px` }"
            />
            <!-- now line -->
            <div
              v-if="sameDay(d, today)"
              class="pointer-events-none absolute inset-x-0 z-10 h-0.5 bg-primary"
              :style="{ top: `${nowLineTop(today, 0, HOUR_HEIGHT)}px` }"
              :title="t('calendar.view.nowAt', { time: nowLabel() })"
            >
              <span class="absolute -left-[5px] -top-1 h-2.5 w-2.5 rounded-full bg-primary" />
            </div>
            <!-- event blocks -->
            <button
              v-for="b in timedBlocks(d)"
              :key="b.ev.id + b.ev.calendar_id"
              type="button"
              class="absolute overflow-hidden rounded px-1 py-0.5 text-left text-2xs leading-tight text-white shadow-sm hover:brightness-110 transition"
              :class="isOngoing(b.ev, today) ? 'ring-[1.5px] ring-info' : 'ring-1 ring-black/10'"
              :style="{
                top: `${b.top}px`,
                height: `${b.height}px`,
                left: `calc(${b.leftPct}% + 1px)`,
                width: `calc(${b.widthPct}% - 2px)`,
                backgroundColor: colorFor(b.ev.account_id),
              }"
              :title="b.ev.title"
              @click.stop="emit('select', b.ev)"
            >
              <div class="font-medium truncate">{{ b.ev.title }}</div>
              <div class="opacity-80 truncate">
                {{ hhmm(b.start) }}<span v-if="isOngoing(b.ev, today)" class="font-medium"> · {{ t('calendar.view.ongoing') }}</span>
              </div>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
