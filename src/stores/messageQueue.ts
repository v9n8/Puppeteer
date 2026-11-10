import { defineStore } from 'pinia'
import { reactive } from 'vue'
import {
  editQueued, emptyQueue, enqueue as enqueueMessage, onTurnEnd, removeQueued, requeue as requeueMessage,
  sanitizeQueue, takeQueued, type MessageQueue, type QueueHold, type QueuedMessage, type TurnEnd,
} from '@/lib/messageQueue'
import type { ChatDraft } from './chatDrafts'

/**
 * Messages queued while a turn runs (UX-25), keyed by SESSION (run id) like
 * chatDrafts, and mirrored to localStorage the same way so switching sessions
 * or reloading keeps them. The rules live in lib/messageQueue.
 *
 * Every webview has its own copy of this store over the one shared
 * localStorage (a session pop-out and the main window can show the same run).
 * Each change therefore reads the stored queues first and writes back from
 * there, so it never overwrites what the other window changed meanwhile, and a
 * `storage` event brings the other window's changes into the chips here. Which
 * window SENDS from a queue is the composer's call (useRunComposer: only the
 * window that takes the run's quotes, see lib/quoteAppend).
 */
const STORAGE_KEY = 'puppeteer.messageQueues'

/** The stored queues, or null when storage can't be read. */
function readStorage(): Record<string, MessageQueue> | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    const obj = raw ? JSON.parse(raw) : {}
    if (!obj || typeof obj !== 'object' || Array.isArray(obj)) return {}
    const out: Record<string, MessageQueue> = {}
    for (const [k, v] of Object.entries(obj)) {
      const q = sanitizeQueue(v)
      if (k && q) out[k] = q
    }
    return out
  } catch {
    return null
  }
}

function newId(): string {
  return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `q-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`
}

export const useMessageQueueStore = defineStore('messageQueue', () => {
  const queues = reactive<Record<string, MessageQueue>>(readStorage() ?? {})

  function replaceAll(next: Record<string, MessageQueue>) {
    for (const k of Object.keys(queues)) if (!(k in next)) delete queues[k]
    Object.assign(queues, next)
  }

  window.addEventListener('storage', (e) => {
    if (e.key !== STORAGE_KEY) return
    const stored = readStorage()
    if (stored) replaceAll(stored)
  })

  /** The run's queue as stored now (this window's copy when storage fails). */
  function fresh(runId: string): MessageQueue {
    const stored = readStorage()
    if (stored) replaceAll(stored)
    return queues[runId] ?? emptyQueue()
  }

  function write(runId: string, q: MessageQueue) {
    if (q.items.length) queues[runId] = q
    else if (runId in queues) delete queues[runId]
    else return
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...queues }))
    } catch {
      /* storage unavailable or quota exceeded — the in-memory queue still works */
    }
  }

  function get(runId: string): MessageQueue {
    return queues[runId] ?? emptyQueue()
  }

  function enqueue(runId: string, draft: ChatDraft) {
    if (runId) write(runId, enqueueMessage(fresh(runId), newId(), draft))
  }

  function edit(runId: string, id: string, text: string) {
    write(runId, editQueued(fresh(runId), id, text))
  }

  function remove(runId: string, id: string) {
    write(runId, removeQueued(fresh(runId), id))
  }

  /** Take one message out to send it by hand. */
  function take(runId: string, id: string): QueuedMessage | null {
    const { queue, item } = takeQueued(fresh(runId), id)
    if (item) write(runId, queue)
    return item
  }

  /** Put a message that was not sent back at the head. */
  function requeue(runId: string, item: QueuedMessage, hold: QueueHold | null) {
    write(runId, requeueMessage(fresh(runId), item, hold))
  }

  /** A turn of the run ended: the message to send now, if any (already dequeued). */
  function turnEnded(runId: string, end: TurnEnd): QueuedMessage | null {
    const { queue, send } = onTurnEnd(fresh(runId), end)
    write(runId, queue)
    return send
  }

  return { queues, get, enqueue, edit, remove, take, requeue, turnEnded }
})
