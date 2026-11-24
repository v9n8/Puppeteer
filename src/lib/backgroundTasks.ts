// Background tasks a session keeps alive after its turn (BG-01/BG-03): a
// backgrounded Bash, a Monitor, a background subagent. While one lives the run
// stays `running` (its process is up) although the turn is over, so "running"
// alone no longer means "a turn is in progress". The backend's snapshot
// (`run:bg_tasks:{id}`, `get_background_tasks`) says which; these pure helpers
// read it for the composer, the queue and the turn status bar. Free of Vue /
// `@/` imports so they can be unit-tested with `node --test`.

export interface BgTask {
  taskId: string
  taskType: string
  description: string
  subagentType: string | null
}

export interface BackgroundState {
  /** The last turn has ended and the live session takes a prompt now. */
  turnDone: boolean
  tasks: BgTask[]
}

/** The backend's snapshot as the store keeps it; null when unusable. */
export function readSnapshot(raw: unknown): BackgroundState | null {
  if (!raw || typeof raw !== 'object') return null
  const o = raw as Record<string, unknown>
  const tasks: BgTask[] = []
  for (const t of Array.isArray(o.tasks) ? o.tasks : []) {
    if (!t || typeof t !== 'object' || typeof t.task_id !== 'string' || !t.task_id) continue
    tasks.push({
      taskId: t.task_id,
      taskType: typeof t.task_type === 'string' ? t.task_type : '',
      description: typeof t.description === 'string' ? t.description : '',
      subagentType: typeof t.subagent_type === 'string' ? t.subagent_type : null,
    })
  }
  return { turnDone: o.turn_done === true, tasks }
}

/**
 * A turn is in progress: the run is live and the backend has not said its turn
 * is over. Enter queues then; otherwise it sends at once.
 */
export function turnBusy(status: string | undefined, bg: BackgroundState | null | undefined): boolean {
  return status === 'running' && !bg?.turnDone
}

/**
 * The status the composer acts on: `running` only while a turn is in progress,
 * `done` for a live session whose turn is over (a message goes out at once).
 */
export function composerStatus(status: string, bg: BackgroundState | null | undefined): string {
  return status === 'running' && !turnBusy(status, bg) ? 'done' : status
}

/** The run is live only for its background tasks: its turn is over and some remain. */
export function waitsOnBackground(status: string | undefined, bg: BackgroundState | null | undefined): boolean {
  return status === 'running' && !!bg?.turnDone && bg.tasks.length > 0
}

/** The live session's turn just ended (the moment the message queue may send). */
export function turnJustEnded(prev: BackgroundState | null | undefined, next: BackgroundState | null | undefined): boolean {
  return !prev?.turnDone && !!next?.turnDone
}

/** A task's one-line label: its description, else its kind. */
export function taskLabel(task: BgTask): string {
  return task.description.replace(/\s+/g, ' ').trim() || task.subagentType || task.taskType || task.taskId
}
