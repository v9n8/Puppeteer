// Sessions linked to the todo open in the task detail panel, and the actions on
// them (open one, link / unlink, "New session from this todo", Copy ID). The
// main window owns sessions: opening or starting one is a request to it
// (WINDOW_OPEN_RUN / WINDOW_NEW_SESSION, App.vue), same as the Notes pane does.
import { ref, watch, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { emit } from '@tauri-apps/api/event'
import { useToast } from '@/composables/useToast'
import { errorCode } from '@/stores/slackThreads'
import {
  WINDOW_NEW_SESSION,
  WINDOW_OPEN_RUN,
  linkReference,
  linkTargetRun,
  listTargetRuns,
  unlinkTargetRun,
  type SessionLink,
  type WindowNewSession,
  type WindowOpenRun,
} from '@/lib/sessionLinks'
import type { FocusTodo } from '@/lib/focus'

export function useTaskLinks(todo: Ref<FocusTodo | null>) {
  const { t } = useI18n()
  const { toast } = useToast()
  const links = ref<SessionLink[]>([])
  const copiedId = ref(false)
  let seq = 0

  async function load() {
    const id = todo.value?.id
    const mine = ++seq
    if (!id) {
      links.value = []
      return
    }
    try {
      const list = await listTargetRuns({ kind: 'todo', id })
      if (mine === seq) links.value = list
    } catch {
      /* an extra: keep what is shown */
    }
  }

  // Another todo, or its link count changed (linked from a session): reload.
  watch(
    () => [todo.value?.id, todo.value?.linked_run_count],
    (now, before) => {
      if (now[0] !== before?.[0]) links.value = []
      void load()
    },
    { immediate: true },
  )

  function openRun(link: SessionLink) {
    emit(WINDOW_OPEN_RUN, { projectId: link.projectId, runId: link.runId } satisfies WindowOpenRun).catch(() => {})
  }

  async function unlinkRun(link: SessionLink) {
    const id = todo.value?.id
    if (!id) return
    const snapshot = links.value.slice()
    links.value = links.value.filter((l) => l.runId !== link.runId)
    try {
      await unlinkTargetRun({ kind: 'todo', id }, link.runId)
    } catch (e) {
      links.value = snapshot
      toast.error(t('sessionLink.unlinkFailed', { error: errorCode(e).detail }))
    }
  }

  async function linkRun(runId: string) {
    const id = todo.value?.id
    if (!id) return
    try {
      await linkTargetRun({ kind: 'todo', id }, runId, 'manual')
      await load()
    } catch (e) {
      toast.error(t('sessionLink.linkFailed', { error: errorCode(e).detail }))
    }
  }

  function newSession() {
    const td = todo.value
    if (!td) return
    if (!td.project_id) {
      toast.info(t('item.needProject'))
      return
    }
    emit(WINDOW_NEW_SESSION, {
      projectId: td.project_id,
      text: linkReference('todo', td.id),
      target: { kind: 'todo', id: td.id },
    } satisfies WindowNewSession).catch((e) => toast.error(String(e)))
  }

  async function copyId() {
    const id = todo.value?.id
    if (!id) return
    try {
      await navigator.clipboard.writeText(id)
      copiedId.value = true
      setTimeout(() => (copiedId.value = false), 1500)
      toast.success(t('item.idCopied'))
    } catch {
      /* clipboard unavailable */
    }
  }

  return { links, copiedId, openRun, unlinkRun, linkRun, newSession, copyId }
}
