<script setup lang="ts">
// Bare, chrome-less host for one run's pop-out permission prompt, shown in a
// standalone OS window (opened via openPermissionWindow / `?permissionWindow=1&runId=…`).
//
// This window is a REMOTE VIEW only: the main window (PermissionWindowManager)
// remains the single source of truth for the permission queue. We render whatever
// it syncs to us and forward the user's decision back — it runs the actual resolve
// logic (shiftPermission + respond_permission), so there is exactly one writer and
// no double-handling. All events are namespaced by runId so each run's window only
// ever sees its own run.
import { computed, onMounted, onBeforeUnmount, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Pin, PinOff, Inbox, ChevronLeft, ChevronRight, ArrowLeft } from 'lucide-vue-next'
import { Button } from '@/components/ui'
// The main window owns the popped-out state and listens to this window's
// 'tauri://destroyed' lifecycle event to re-dock, so this window doesn't need
// to announce its own close.
import PermissionPrompt, { type PermissionRequest } from '@/components/PermissionPrompt.vue'
import PermissionRequestBody from '@/components/permission/PermissionRequestBody.vue'
import { useMarkdown } from '@/lib/markdown'
const { t } = useI18n()

const { renderText, loadMarkdown } = useMarkdown()

// Which run this window answers for; every event is scoped to it.
const runId = new URLSearchParams(window.location.search).get('runId') ?? ''

interface QueueItem {
  request: PermissionRequest
  purpose: string | null
}

const request = ref<PermissionRequest | null>(null)
const purpose = ref<string | null>(null)
const queue = ref<QueueItem[]>([])
// Which queued request is on screen. Anything but 0 is view only: the decision
// buttons answer the head request, so they are swapped for "back" there.
const viewIndex = ref(0)
const viewed = computed(() => (viewIndex.value > 0 ? queue.value[viewIndex.value] ?? null : null))
const viewedIsTool = computed(() => !['AskUserQuestion', 'ExitPlanMode'].includes(viewed.value?.request.tool_name ?? ''))
const allowedTools = ref<string[]>([])
// Mirrors the window's always-on-top flag (opened with alwaysOnTop: true).
const pinned = ref(true)

let syncUnlisten: UnlistenFn | null = null
// Re-announce readiness until the main window has synced us once: a missed
// first `window-ready` (it raced the main window subscribing — more likely in a
// remote-desktop window, where events cross windows over a local bus) would
// otherwise leave this window on "waiting" for good.
let readyTimer: ReturnType<typeof setInterval> | null = null
let synced = false

// Which session is asking — shown in the header and the OS title so several
// permission windows open at once can be told apart.
const session = ref<{ name: string; project: string } | null>(null)

// Main → popup: the current head request (or null when the queue is empty).
interface SyncPayload {
  request: PermissionRequest | null
  purpose?: string | null
  /** Every waiting request, head first (view only). */
  queue?: QueueItem[]
  allowedTools: string[]
  session?: { name: string; project: string }
}

async function applyTitle() {
  const s = session.value
  const name = s ? (s.project && s.name !== s.project ? `${s.name} — ${s.project}` : s.name) : ''
  const title = name ? `${t('permission.window.osTitlePrefix')}: ${name}` : t('permission.window.title')
  document.title = title
  try {
    await getCurrentWindow().setTitle(title)
  } catch {
    /* window api unavailable */
  }
}

onMounted(async () => {
  document.title = t('permission.window.title')
  loadMarkdown()

  syncUnlisten = await listen<SyncPayload>(`permission:sync:${runId}`, (event) => {
    synced = true
    if (readyTimer) {
      clearInterval(readyTimer)
      readyTimer = null
    }
    const head = event.payload?.request ?? null
    if (head?.request_id !== request.value?.request_id) viewIndex.value = 0
    request.value = head
    purpose.value = event.payload?.purpose ?? null
    queue.value = event.payload?.queue ?? (head ? [{ request: head, purpose: purpose.value }] : [])
    if (viewIndex.value >= queue.value.length) viewIndex.value = 0
    allowedTools.value = event.payload?.allowedTools ?? []
    const next = event.payload?.session ?? null
    if (next?.name !== session.value?.name || next?.project !== session.value?.project) {
      session.value = next
      void applyTitle()
    }
  })

  // Tell the main window we're ready so it (re)sends the current state — covers
  // the case where the request arrived before this window finished mounting.
  await emit(`permission:window-ready:${runId}`)
  let tries = 0
  readyTimer = setInterval(() => {
    if (synced || ++tries > 8) {
      if (readyTimer) clearInterval(readyTimer)
      readyTimer = null
      return
    }
    void emit(`permission:window-ready:${runId}`)
  }, 1500)
})

onBeforeUnmount(() => {
  if (readyTimer) clearInterval(readyTimer)
  readyTimer = null
  syncUnlisten?.()
  syncUnlisten = null
})

function onDecide(decision: 'allow' | 'deny', remember: boolean) {
  const req = request.value
  if (!req) return
  emit(`permission:decide:${runId}`, { request_id: req.request_id, decision, remember })
  // Optimistically clear so the panel doesn't linger; the main window will sync
  // the next request (or null) right after it resolves this one.
  request.value = null
  queue.value = queue.value.slice(1)
}

function onAnswer(answers: Record<string, string>) {
  const req = request.value
  if (!req) return
  emit(`permission:answer:${runId}`, { request_id: req.request_id, answers })
  request.value = null
  queue.value = queue.value.slice(1)
}

async function togglePin() {
  pinned.value = !pinned.value
  try {
    await getCurrentWindow().setAlwaysOnTop(pinned.value)
  } catch {
    /* window api unavailable */
  }
}
</script>

<template>
  <div class="flex h-screen w-screen flex-col bg-background text-foreground overflow-hidden">
    <!-- Slim titlebar: pin toggle so the user can drop always-on-top if it
         gets in the way while comparing against the chat on another monitor. -->
    <div class="flex items-center gap-2 px-3 h-11 border-b border-border/60 shrink-0">
      <div class="flex min-w-0 flex-col leading-tight">
        <!-- Project on top, session name below; generic label until the first sync. -->
        <span class="truncate text-2xs uppercase tracking-wide text-foreground/50" :title="session?.project">
          {{ session?.project || t('permission.window.headerLabel') }}
        </span>
        <span v-if="session" class="truncate text-xs font-medium text-foreground/85" :title="session.name">
          {{ session.name }}
        </span>
      </div>
      <!-- Several requests waiting for this session: page through them (view only). -->
      <div v-if="queue.length > 1" class="ml-auto flex shrink-0 items-center gap-0.5 text-xs text-foreground/70">
        <Button
          variant="ghost"
          size="icon-sm"
          :disabled="viewIndex === 0"
          :title="t('permission.prevRequest')"
          :aria-label="t('permission.prevRequest')"
          @click="viewIndex--"
        >
          <ChevronLeft class="h-4 w-4" :stroke-width="1.75" />
        </Button>
        <span class="min-w-8 text-center tabular-nums">{{ viewIndex + 1 }}/{{ queue.length }}</span>
        <Button
          variant="ghost"
          size="icon-sm"
          :disabled="viewIndex >= queue.length - 1"
          :title="t('permission.nextRequest')"
          :aria-label="t('permission.nextRequest')"
          @click="viewIndex++"
        >
          <ChevronRight class="h-4 w-4" :stroke-width="1.75" />
        </Button>
      </div>
      <Button
        :variant="pinned ? 'primary' : 'outline'"
        size="xs"
        :class="queue.length > 1 ? 'shrink-0' : 'ml-auto shrink-0'"
        :aria-pressed="pinned"
        :title="pinned ? t('permission.window.pinnedTitle') : t('permission.window.pinTitle')"
        @click="togglePin"
      >
        <component :is="pinned ? Pin : PinOff" class="h-3 w-3" :stroke-width="1.75" />
        {{ pinned ? t('permission.window.pinned') : t('permission.window.pin') }}
      </Button>
    </div>

    <!-- Pending request → the shared prompt; empty → waiting state. -->
    <div class="flex-1 min-h-0 overflow-auto">
      <!-- A queued request that isn't first: read-only, decisions stay on the head. -->
      <div v-if="viewed" class="flex h-full min-h-0 flex-col border-t-2 border-t-warning/70 bg-card">
        <div class="min-h-0 flex-1 space-y-3 overflow-auto px-5 py-4">
          <h2 class="text-sm font-medium text-foreground">{{ viewed.request.title || viewed.request.display_name || viewed.request.tool_name }}</h2>
          <PermissionRequestBody v-if="viewedIsTool" :request="viewed.request" :purpose="viewed.purpose" />
        </div>
        <div class="flex shrink-0 flex-col gap-2 border-t border-border px-5 py-3">
          <p class="text-2xs text-foreground/40">{{ t('permission.viewOnly') }}</p>
          <Button variant="primary" @click="viewIndex = 0">
            <ArrowLeft class="h-3.5 w-3.5" :stroke-width="2" /> {{ t('permission.backToPending') }}
          </Button>
        </div>
      </div>
      <PermissionPrompt
        v-else-if="request"
        :key="request.request_id"
        :request="request"
        :purpose="purpose"
        :allowed-tools="allowedTools"
        :render-text="renderText"
        @decide="onDecide"
        @answer="onAnswer"
      />
      <div v-else class="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <Inbox class="h-8 w-8 text-foreground/20" :stroke-width="1" />
        <p class="text-xs text-foreground/40">
          {{ t('permission.window.emptyTitle') }}<br />
          {{ t('permission.window.emptyBody') }}
        </p>
      </div>
    </div>
  </div>
</template>
