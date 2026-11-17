<script setup lang="ts">
// The head permission request / question of a run, answered right under the
// conversation. The permission window stays as it is (QDR-08); answering in either
// place makes the other one go away.
//
// Main window: reads the shared liveRuns queue and calls the single decision writer
// (lib/permissionDecision) directly. Popped-out session window: its own store does
// not learn about answers given elsewhere, so it mirrors the main window's
// `permission:sync` and forwards the choice as `permission:decide|answer`, like
// PermissionWindow.vue does; the main window's manager writes it.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import PermissionPrompt, { type PermissionRequest } from '@/components/PermissionPrompt.vue'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { decidePermission, answerPermission } from '@/lib/permissionDecision'
import { permissionPurpose } from '@/lib/permissionImpact'
import { IS_SESSION_WINDOW } from '@/lib/windowMode'
import { useMarkdown } from '@/lib/markdown'

const props = defineProps<{ runId: string }>()
const { t } = useI18n()
const live = useLiveRunsStore()
const { renderText, loadMarkdown } = useMarkdown()

// Session window only: the last state the main window synced to us.
const synced = ref<{
  request: PermissionRequest | null
  purpose: string | null
  count: number
  allowedTools: string[]
}>({ request: null, purpose: null, count: 0, allowedTools: [] })

const session = computed(() => live.get(props.runId))
const request = computed<PermissionRequest | null>(() =>
  IS_SESSION_WINDOW ? synced.value.request : session.value?.permissionQueue[0] ?? null,
)
const purpose = computed(() =>
  IS_SESSION_WINDOW
    ? synced.value.purpose
    : request.value
      ? permissionPurpose(request.value, session.value?.entries)
      : null,
)
const count = computed(() =>
  IS_SESSION_WINDOW ? synced.value.count : session.value?.permissionQueue.length ?? 0,
)
const allowedTools = computed(() =>
  IS_SESSION_WINDOW ? synced.value.allowedTools : session.value?.allowedTools ?? [],
)

function onDecide(decision: 'allow' | 'deny', remember: boolean) {
  const req = request.value
  if (!req) return
  if (!IS_SESSION_WINDOW) {
    // On failure the request stays in the queue, so the block stays for a retry.
    void decidePermission(live, props.runId, req.request_id, decision, remember)
    return
  }
  void emit(`permission:decide:${props.runId}`, { request_id: req.request_id, decision, remember })
  synced.value = { ...synced.value, request: null } // the next sync brings the next request
}

function onAnswer(answers: Record<string, string>) {
  const req = request.value
  if (!req) return
  if (!IS_SESSION_WINDOW) {
    void answerPermission(live, props.runId, req.request_id, answers)
    return
  }
  void emit(`permission:answer:${props.runId}`, { request_id: req.request_id, answers })
  synced.value = { ...synced.value, request: null }
}

let off: UnlistenFn | null = null
let readyTimer: ReturnType<typeof setInterval> | null = null
let gotSync = false
let subscribeSeq = 0

function stopListening() {
  subscribeSeq++
  if (readyTimer) clearInterval(readyTimer)
  readyTimer = null
  off?.()
  off = null
}

onMounted(() => {
  loadMarkdown()
  if (IS_SESSION_WINDOW) void startListening(props.runId)
})

// The session window can switch to another run (engine change); listen to the new one.
watch(
  () => props.runId,
  (id) => {
    if (!IS_SESSION_WINDOW) return
    stopListening()
    gotSync = false
    synced.value = { request: null, purpose: null, count: 0, allowedTools: [] }
    void startListening(id)
  },
)

async function startListening(runId: string) {
  const seq = ++subscribeSeq
  const unlisten = await listen<{
    request: PermissionRequest | null
    purpose?: string | null
    queue?: unknown[]
    allowedTools?: string[]
  }>(`permission:sync:${runId}`, (e) => {
    gotSync = true
    const p = e.payload
    synced.value = {
      request: p?.request ?? null,
      purpose: p?.purpose ?? null,
      count: p?.queue?.length ?? (p?.request ? 1 : 0),
      allowedTools: p?.allowedTools ?? [],
    }
  })
  if (seq !== subscribeSeq) {
    unlisten() // the run changed or the block went away while subscribing
    return
  }
  off = unlisten
  // Ask the main window for the current state (it only listens while a request is
  // pending, so a missed ping is retried briefly).
  void emit(`permission:window-ready:${runId}`)
  let tries = 0
  readyTimer = setInterval(() => {
    if (gotSync || ++tries > 5) {
      if (readyTimer) clearInterval(readyTimer)
      readyTimer = null
      return
    }
    void emit(`permission:window-ready:${runId}`)
  }, 1500)
}

onBeforeUnmount(stopListening)
</script>

<template>
  <div
    v-if="request"
    data-inline-permission
    class="flex max-h-[55%] min-h-0 shrink-0 flex-col overflow-hidden border-t border-border"
    role="region"
    :aria-label="t('permission.inline.aria')"
  >
    <div v-if="count > 1" class="shrink-0 bg-card px-5 pt-2 text-2xs font-medium tabular-nums text-foreground/50">
      1/{{ count }}
    </div>
    <PermissionPrompt
      :key="request.request_id"
      :request="request"
      :purpose="purpose"
      :allowed-tools="allowedTools"
      :render-text="renderText"
      layout="inline"
      :autofocus="false"
      @decide="onDecide"
      @answer="onAnswer"
    />
  </div>
</template>
