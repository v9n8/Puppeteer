<script setup lang="ts">
// The turn status bar above the composer (V3-17): one 32px line that says what
// the session's last turn is doing, and a list above it, opened by a click, with
// the turn's steps (and how long each took) and its todo. Waiting for permission
// or an answer, the click does that job instead (the chevron still opens the
// list). State comes from useTurnStatus; the choice of status is lib/turnStatus.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  Ban, Check, ChevronUp, Circle, CircleDot, Loader2, MessageCircleQuestion, ShieldAlert, X,
} from 'lucide-vue-next'
import { formatActivityDuration } from '@/lib/turnActivity'
import { formatClock } from '@/lib/slackThreadUtils'
import { engineLabel } from '@/lib/skillRows'
import { openPermissionWindow } from '@/lib/permissionWindow'
import { useTurnStatus } from './useTurnStatus'

const { t, locale } = useI18n()
const { currentRunId, currentRun, status, steps, todos, todo, stoppedAt, now } = useTurnStatus()

const open = ref(false)
const root = ref<HTMLElement | null>(null)

// AC-12: another session starts closed.
watch(currentRunId, () => { open.value = false })

const visible = computed(() => status.value.kind !== 'none')
const listId = 'turn-status-list'

const clock = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000))
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}
const seconds = (ms: number) => {
  const text = formatActivityDuration(ms)
  return locale.value.startsWith('vi') ? text.replace('.', ',') : text
}
const count = (n: number) => new Intl.NumberFormat(locale.value).format(n)

const tone = computed(() => {
  const k = status.value.kind
  if (k === 'permission' || k === 'question') return 'border-warning/40 bg-warning/10 text-warning'
  if (k === 'error') return 'border-destructive/40 bg-destructive/10 text-destructive'
  return 'border-border bg-background text-foreground'
})

const todoText = () => (todo.value.total ? t('run.turnStatus.todo', todo.value) : '')

// The bar's text in parts: a sentence, and the mono detail after it.
const parts = computed<{ text: string; detail: string; extra: string; todo: string }>(() => {
  const s = status.value
  switch (s.kind) {
    case 'done': {
      const bits = [t('run.turnStatus.done', { n: s.turn })]
      if (s.durationMs) bits.push(seconds(s.durationMs))
      if (s.tokens) bits.push(t('run.turnStatus.tokens', { n: count(s.tokens) }))
      return { text: bits.join(' · '), detail: '', extra: '', todo: '' }
    }
    case 'tool': {
      const ms = s.startedAt !== null ? now.value - s.startedAt : s.elapsedSecs !== null ? s.elapsedSecs * 1000 : null
      return {
        text: t('run.turnStatus.running', { tool: s.tool }),
        detail: s.detail,
        extra: ms !== null ? clock(ms) : '',
        todo: todoText(),
      }
    }
    case 'thinking': {
      const retry = s.retry
        ? t('run.turnStatus.retry', { attempt: s.retry.attempt, max: s.retry.maxRetries, secs: s.retry.delaySecs })
        : ''
      return { text: t('run.turnStatus.thinking'), detail: '', extra: retry, todo: todoText() }
    }
    case 'permission':
      return { text: t('run.turnStatus.permission'), detail: s.detail, extra: '', todo: '' }
    case 'question':
      return { text: t('run.turnStatus.asking', { engine: engineLabel(currentRun.value?.engine || 'claude') }), detail: s.text, extra: '', todo: '' }
    case 'error':
      return { text: s.text ? `${t('run.turnStatus.failed')}: ${s.text}` : t('run.turnStatus.failed'), detail: '', extra: '', todo: '' }
    case 'cancelled': {
      const at = stoppedAt.value ? formatClock(stoppedAt.value) : ''
      return {
        text: at ? t('run.turnStatus.cancelledAt', { n: s.turn, time: at }) : t('run.turnStatus.cancelled', { n: s.turn }),
        detail: '',
        extra: '',
        todo: '',
      }
    }
    default:
      return { text: '—', detail: '', extra: '', todo: '' }
  }
})

// Waiting for the user: the bar's click does the job; null = it opens the list.
function act(): (() => void) | null {
  const k = status.value.kind
  if (k === 'permission' && currentRunId.value) {
    const id = currentRunId.value
    return () => { void openPermissionWindow(id) }
  }
  if (k === 'question') {
    return () => {
      document.querySelector('[data-inline-permission]')?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
    }
  }
  return null
}
const action = computed(() => act())
const mainTitle = computed(() => {
  const k = status.value.kind
  if (k === 'permission') return t('run.turnStatus.permissionTitle')
  if (k === 'question') return t('run.turnStatus.askingTitle')
  return t('run.turnStatus.tooltip')
})

function onMain() {
  if (action.value) action.value()
  else open.value = !open.value
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && open.value) {
    open.value = false
    e.stopPropagation()
  }
}
function onPointerDown(e: PointerEvent) {
  if (open.value && root.value && !root.value.contains(e.target as Node)) open.value = false
}
// Esc is heard on the document: WebKit does not focus a button on mouse click,
// so a list opened by mouse has no focus inside the block to receive the key.
onMounted(() => {
  document.addEventListener('pointerdown', onPointerDown)
  document.addEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onPointerDown)
  document.removeEventListener('keydown', onKeydown)
})

const stepsTitle = computed(() => t('run.turnStatus.stepsTitle', { n: status.value.kind === 'none' || status.value.kind === 'loading' ? 0 : status.value.turn, count: steps.value.length }))
const stepTime = (s: (typeof steps.value)[number]) => {
  if (s.state === 'running') return s.startedAt !== null ? clock(now.value - s.startedAt) : '…'
  return s.durationMs !== null ? seconds(s.durationMs) : '—'
}
const todoIconClass = { completed: 'text-emerald-500', in_progress: 'text-primary', pending: 'text-foreground/35' } as const
</script>

<template>
  <div v-if="visible" ref="root" class="relative shrink-0">
    <!-- The list opens upward, over the conversation. -->
    <div
      v-if="open"
      :id="listId"
      class="absolute inset-x-0 bottom-full z-20 grid max-h-72 min-w-0 grid-cols-1 gap-x-4 gap-y-3 overflow-y-auto border-t border-border bg-card p-3 shadow-lg lg:grid-cols-2"
    >
      <section class="min-w-0">
        <h3 class="mb-1.5 text-2xs font-medium uppercase tracking-wide text-foreground/50">{{ stepsTitle }}</h3>
        <p v-if="!steps.length" class="text-2xs text-foreground/50">{{ t('run.turnStatus.noSteps') }}</p>
        <ul v-else class="space-y-0.5">
          <li v-for="s in steps" :key="s.at" class="flex min-h-7 min-w-0 items-center gap-2 text-2xs">
            <Loader2 v-if="s.state === 'running'" class="h-3.5 w-3.5 shrink-0 animate-spin text-primary" :stroke-width="2" />
            <X v-else-if="s.state === 'error'" class="h-3.5 w-3.5 shrink-0 text-destructive" :stroke-width="2" />
            <Check v-else class="h-3.5 w-3.5 shrink-0 text-emerald-500" :stroke-width="2" />
            <span class="w-14 shrink-0 truncate text-foreground/70">{{ s.kind === 'other' ? s.name : t(`run.turnStatus.kind.${s.kind}`) }}</span>
            <code class="min-w-0 flex-1 truncate font-mono text-foreground/60" :title="s.detail">{{ s.detail }}</code>
            <span class="shrink-0 font-mono tabular-nums text-foreground/50">{{ stepTime(s) }}</span>
          </li>
        </ul>
      </section>
      <section class="min-w-0">
        <h3 class="mb-1.5 text-2xs font-medium uppercase tracking-wide text-foreground/50">
          {{ t('run.turnStatus.todoTitle', todo) }}
        </h3>
        <p v-if="!todos.length" class="text-2xs text-foreground/50">{{ t('run.turnStatus.noTodo') }}</p>
        <ul v-else class="space-y-0.5">
          <li v-for="(item, i) in todos" :key="i" class="flex min-h-7 min-w-0 items-center gap-2 text-2xs">
            <Check
              v-if="item.status === 'completed'"
              :class="['h-3.5 w-3.5 shrink-0', todoIconClass.completed]"
              :stroke-width="2"
              :aria-label="t('run.turnStatus.todoDone')"
            />
            <CircleDot
              v-else-if="item.status === 'in_progress'"
              :class="['h-3.5 w-3.5 shrink-0', todoIconClass.in_progress]"
              :stroke-width="2"
              :aria-label="t('run.turnStatus.todoDoing')"
            />
            <Circle
              v-else
              :class="['h-3.5 w-3.5 shrink-0', todoIconClass.pending]"
              :stroke-width="2"
              :aria-label="t('run.turnStatus.todoPending')"
            />
            <span class="min-w-0 flex-1 truncate" :class="item.status === 'completed' ? 'text-foreground/45 line-through' : 'text-foreground/80'" :title="item.content">
              {{ item.content }}
            </span>
          </li>
        </ul>
      </section>
    </div>

    <div class="flex h-8 min-w-0 items-stretch border-t" :class="tone" role="status" :aria-label="t('run.turnStatus.label')">
      <button
        type="button"
        class="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 px-4 text-left text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
        :title="mainTitle"
        :aria-expanded="action ? undefined : open"
        :aria-controls="action ? undefined : listId"
        @click="onMain"
      >
        <Loader2 v-if="status.kind === 'tool' || status.kind === 'thinking'" class="h-3.5 w-3.5 shrink-0 animate-spin text-primary" :stroke-width="2" />
        <Check v-else-if="status.kind === 'done'" class="h-3.5 w-3.5 shrink-0 text-emerald-500" :stroke-width="2" />
        <ShieldAlert v-else-if="status.kind === 'permission'" class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
        <MessageCircleQuestion v-else-if="status.kind === 'question'" class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
        <X v-else-if="status.kind === 'error'" class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
        <Ban v-else-if="status.kind === 'cancelled'" class="h-3.5 w-3.5 shrink-0 text-foreground/50" :stroke-width="2" />
        <span class="shrink-0 truncate" :class="status.kind === 'tool' || status.kind === 'thinking' || status.kind === 'done' ? 'max-w-[60%]' : 'max-w-[50%]'">{{ parts.text }}</span>
        <code v-if="parts.detail" class="min-w-0 flex-1 truncate font-mono opacity-80">{{ parts.detail }}</code>
        <span v-else class="flex-1" />
        <span v-if="parts.extra" class="shrink-0 truncate font-mono tabular-nums text-foreground/55">{{ parts.extra }}</span>
        <span v-if="parts.todo" class="shrink-0 truncate font-mono tabular-nums text-foreground/55 max-[1000px]:hidden">· {{ parts.todo }}</span>
        <ChevronUp
          v-if="!action"
          class="h-3.5 w-3.5 shrink-0 text-foreground/45 transition-transform"
          :class="{ 'rotate-180': open }"
          :stroke-width="2"
        />
      </button>
      <button
        v-if="action"
        type="button"
        class="flex w-8 shrink-0 cursor-pointer items-center justify-center focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
        :title="t('run.turnStatus.tooltip')"
        :aria-expanded="open"
        :aria-controls="listId"
        @click="open = !open"
      >
        <ChevronUp class="h-3.5 w-3.5 transition-transform" :class="{ 'rotate-180': open }" :stroke-width="2" />
      </button>
    </div>
  </div>
</template>
