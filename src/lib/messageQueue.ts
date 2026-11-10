// Messages queued while a turn is running (UX-25): the pure state machine behind
// the composer's Queue button and the chips above it. The store
// (stores/messageQueue) keeps one queue per run and persists it; the composer
// sends what `onTurnEnd` hands back.
//
// A queued item is a snapshot of the composer draft at the moment it was queued
// (text, pasted images, attached files). Quotes are `> ` text inside that draft
// today, so the snapshot already carries them; once drafts hold structured
// quotes (UX-29), they belong in the same snapshot.
import type { ChatDraft } from '@/stores/chatDrafts'

export interface QueuedMessage {
  id: string
  draft: ChatDraft
}

/**
 * Why the queue did not send at the last turn end. The chips stay and the
 * composer says why; the next successful turn end, or a manual send, clears it.
 */
export type QueueHold = 'failed' | 'cancelled' | 'budget'

export interface MessageQueue {
  items: QueuedMessage[]
  hold: QueueHold | null
}

export function emptyQueue(): MessageQueue {
  return { items: [], hold: null }
}

function draftIsEmpty(d: ChatDraft): boolean {
  return !d.text.trim() && d.images.length === 0 && d.files.length === 0
}

function copyDraft(d: ChatDraft): ChatDraft {
  return { text: d.text, images: [...d.images], files: [...d.files] }
}

/** Add a draft at the end. An empty draft is not a message, so it is ignored. */
export function enqueue(q: MessageQueue, id: string, draft: ChatDraft): MessageQueue {
  if (draftIsEmpty(draft)) return q
  return { ...q, items: [...q.items, { id, draft: copyDraft(draft) }] }
}

/** Change a queued message's text. One left with nothing to send is dropped. */
export function editQueued(q: MessageQueue, id: string, text: string): MessageQueue {
  const items: QueuedMessage[] = []
  for (const item of q.items) {
    if (item.id !== id) {
      items.push(item)
      continue
    }
    const draft = { ...copyDraft(item.draft), text }
    if (!draftIsEmpty(draft)) items.push({ id, draft })
  }
  return { ...q, items }
}

export function removeQueued(q: MessageQueue, id: string): MessageQueue {
  return { ...q, items: q.items.filter((i) => i.id !== id) }
}

/**
 * Take one message out to send it now (the chip's Send button). Sending by hand
 * answers whatever held the queue, so the hold goes too.
 */
export function takeQueued(q: MessageQueue, id: string): { queue: MessageQueue; item: QueuedMessage | null } {
  const item = q.items.find((i) => i.id === id) ?? null
  if (!item) return { queue: q, item: null }
  return { queue: { items: q.items.filter((i) => i.id !== id), hold: null }, item }
}

/**
 * Put a message that was not sent back at the head: held for `hold`, or (null)
 * waiting for the next turn end, e.g. when another send got there first.
 */
export function requeue(q: MessageQueue, item: QueuedMessage, hold: QueueHold | null): MessageQueue {
  return { items: [item, ...q.items.filter((i) => i.id !== item.id)], hold }
}

export interface TurnEnd {
  /** The run's status from `run:done`: done / failed / cancelled. */
  status: string
  /** The budget guard would refuse the next turn right now. */
  overBudget: boolean
}

/**
 * A turn of the run ended. A successful turn sends the head of the queue, one
 * per turn end, as a normal follow-up; the rest wait for the turn it starts. A
 * failed or cancelled turn, or a blocked budget, keeps every message and holds
 * the queue so the user decides.
 */
export function onTurnEnd(q: MessageQueue, end: TurnEnd): { queue: MessageQueue; send: QueuedMessage | null } {
  if (!q.items.length) return { queue: q.hold ? emptyQueue() : q, send: null }
  let hold: QueueHold | null
  if (end.status === 'failed') hold = 'failed'
  else if (end.status === 'cancelled') hold = 'cancelled'
  else if (end.status !== 'done') return { queue: q, send: null }
  else if (end.overBudget) hold = 'budget'
  else {
    const [send, ...rest] = q.items
    return { queue: { items: rest, hold: null }, send }
  }
  return { queue: { ...q, hold }, send: null }
}

const HOLDS: readonly string[] = ['failed', 'cancelled', 'budget']

function sanitizeDraft(raw: unknown): ChatDraft | null {
  if (!raw || typeof raw !== 'object') return null
  const obj = raw as Record<string, unknown>
  const images = Array.isArray(obj.images)
    ? obj.images.filter(
        (i): i is ChatDraft['images'][number] =>
          !!i && typeof i.media_type === 'string' && typeof i.data === 'string',
      ).map((i) => ({ media_type: i.media_type, data: i.data }))
    : []
  const files = Array.isArray(obj.files)
    ? obj.files.filter(
        (f): f is ChatDraft['files'][number] => !!f && typeof f.name === 'string' && typeof f.path === 'string',
      ).map((f) => ({ name: f.name, path: f.path }))
    : []
  const draft = { text: typeof obj.text === 'string' ? obj.text : '', images, files }
  return draftIsEmpty(draft) ? null : draft
}

/** Read a persisted queue back. Null when nothing usable is left. */
export function sanitizeQueue(raw: unknown): MessageQueue | null {
  if (!raw || typeof raw !== 'object') return null
  const obj = raw as Record<string, unknown>
  const items: QueuedMessage[] = []
  if (Array.isArray(obj.items)) {
    for (const it of obj.items) {
      if (!it || typeof it !== 'object' || typeof it.id !== 'string' || !it.id) continue
      if (items.some((i) => i.id === it.id)) continue
      const draft = sanitizeDraft(it.draft)
      if (draft) items.push({ id: it.id, draft })
    }
  }
  if (!items.length) return null
  const hold = typeof obj.hold === 'string' && HOLDS.includes(obj.hold) ? (obj.hold as QueueHold) : null
  return { items, hold }
}

/**
 * One-line label for a chip: the first line the user typed, else (a message
 * that is only a quote) the first quoted line without its `>`.
 */
export function queuedPreview(item: QueuedMessage): string {
  const lines = item.draft.text.split('\n').map((l) => l.trim()).filter(Boolean)
  const typed = lines.find((l) => !l.startsWith('>'))
  if (typed) return typed
  return lines.map((l) => l.replace(/^>\s?/, '').trim()).find(Boolean) ?? ''
}
