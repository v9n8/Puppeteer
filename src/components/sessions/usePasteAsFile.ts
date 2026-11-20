// Long pastes as a `.txt` attachment (#16, RT-26). A text paste longer than the
// project's threshold (Project › Session defaults) is written by the backend to
// `<app data>/pastes/<run id>/paste-<time>.txt` (`save_pasted_text`, RT-25) and
// attached like a picked file; a toast says so, with "Edit" opening that setting.
//
// - No run yet (a new session not started): the paste stays plain text. The file
//   lives in the run's folder (removed with the session), and creating a run on a
//   paste would start a session the user has not sent.
// - Saving fails: an error toast and the text goes into the draft after all, so
//   nothing pasted is lost.
import { emit as emitEvent } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute } from '@/lib/detachedScreens'
import { projectPageQuery } from '@/lib/projectSections'
import { pasteFileName, shouldPasteAsFile } from '@/lib/pasteAsFile'
import type { RunSessionCore } from './useRunSessionCore'

export function usePasteAsFile(core: RunSessionCore, addPendingFile: (path: string) => void) {
  const { t, toast, router, project, projectId, currentRunId, isSessionWindow } = core

  function openSetting() {
    const path = `/projects?${new URLSearchParams(projectPageQuery(projectId.value, 'session'))}`
    // A session pop-out keeps its one run: the main window opens the page.
    if (isSessionWindow) void emitEvent(SCREEN_OPEN_ROUTE, { path } satisfies ScreenOpenRoute)
    else void router.push(path)
  }

  /** Put `text` into the textarea at its caret, as the paste would have. */
  function insertIntoDraft(el: HTMLTextAreaElement | null, text: string) {
    if (!el) return
    el.setRangeText(text, el.selectionStart, el.selectionEnd, 'end')
    el.dispatchEvent(new Event('input', { bubbles: true })) // updates the v-model
  }

  async function save(runId: string, text: string, el: HTMLTextAreaElement | null) {
    try {
      const path = await invoke<string>('save_pasted_text', { runId, text })
      addPendingFile(path)
      toast.success(
        t('run.pasteAsFile.saved', { name: pasteFileName(path), count: text.length }),
        6000,
        { label: t('common.edit'), run: openSetting },
      )
    } catch (e) {
      toast.error(t('run.pasteAsFile.failed', { error: String(e) }))
      insertIntoDraft(el, text)
    }
  }

  /**
   * Called for a paste that carried no image. True when the text was taken (the
   * caller then stops the browser from inserting it).
   */
  function onPaste(e: ClipboardEvent): boolean {
    const runId = currentRunId.value
    if (!runId) return false
    const text = e.clipboardData?.getData('text/plain') ?? ''
    if (!text || !shouldPasteAsFile(text, project.value?.paste_as_file)) return false
    const el = e.target instanceof HTMLTextAreaElement ? e.target : null
    void save(runId, text, el)
    return true
  }

  return { onPaste }
}
