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
//
// ⌘⇧V (macOS) / Ctrl+Shift+V pastes the other way: the opposite of the setting,
// whatever the length (`pasteAction`). macOS fires no `paste` event for it, so the
// keydown reads the clipboard itself (`read_clipboard_text`, local-only: a remote
// window reads the controlling machine's clipboard, never the host's).
import { emit as emitEvent } from '@tauri-apps/api/event'
import { invoke as localInvoke } from '@tauri-real/core'
import { invoke } from '@/lib/tauri'
import { IS_MAC } from '@/lib/appMenu'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute } from '@/lib/detachedScreens'
import { projectPageQuery } from '@/lib/projectSections'
import { isInvertPasteKey, pasteAction, pasteFileName } from '@/lib/pasteAsFile'
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

  // WINDOWS-LINUX-REVIEW: the one-shot "invert the next paste" flag assumes WebView2 / WebKitGTK
  // fire a native `paste` event for Ctrl+Shift+V right after its keydown; unverified.
  let invertNext = false
  const clearInvert = () => { invertNext = false }

  /** Where `text` goes, as `pasteAction` decides; true when it was taken from the browser. */
  function place(text: string, inverted: boolean, el: HTMLTextAreaElement | null, insert: boolean): boolean {
    const runId = currentRunId.value
    const action = pasteAction(text, project.value?.paste_as_file, { inverted, hasRun: !!runId })
    if (action === 'file' && runId) {
      void save(runId, text, el)
      return true
    }
    if (action === 'draft_no_run') toast.info(t('run.pasteAsFile.needsRun'), 5000)
    if (insert) insertIntoDraft(el, text)
    return false
  }

  /**
   * Called for a paste that carried no image. True when the text was taken (the
   * caller then stops the browser from inserting it).
   */
  function onPaste(e: ClipboardEvent): boolean {
    // WINDOWS-LINUX-REVIEW: consumes the flag Ctrl+Shift+V set; unverified that this paste is that one.
    const inverted = invertNext
    clearInvert()
    const text = e.clipboardData?.getData('text/plain') ?? ''
    if (!text) return false
    return place(text, inverted, e.target instanceof HTMLTextAreaElement ? e.target : null, false)
  }

  async function pasteFromClipboard(el: HTMLTextAreaElement | null) {
    let text: string | null
    try {
      text = await localInvoke<string | null>('read_clipboard_text')
    } catch (e) {
      toast.error(t('run.pasteAsFile.readFailed', { error: String(e) }))
      return
    }
    if (text) place(text, true, el, true)
  }

  /**
   * The composer's keydown, first: true when it was the "paste the other way" shortcut
   * (the caller then stops). Any other key clears a pending one-shot flag.
   */
  function onShortcut(e: KeyboardEvent): boolean {
    if (!isInvertPasteKey(e, IS_MAC)) {
      clearInvert()
      return false
    }
    const el = e.target instanceof HTMLTextAreaElement ? e.target : null
    if (IS_MAC) {
      e.preventDefault()
      void pasteFromClipboard(el)
      return true
    }
    // WINDOWS-LINUX-REVIEW: no preventDefault, so the native paste still fires and reads the flag;
    // the flag also clears on blur. Unverified on WebView2 / WebKitGTK.
    invertNext = true
    el?.addEventListener('blur', clearInvert, { once: true })
    return true
  }

  return { onPaste, onShortcut }
}
