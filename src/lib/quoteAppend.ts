// Quote from a window without a composer (the file pop-out) into a session's
// draft, as the `quote:append` event.
//
// Each webview has its own chatDrafts store, mirrored to localStorage, so two
// windows taking the same quote would double it or clobber each other's copy.
// The sender therefore names ONE writer: the run's session pop-out when it is
// open (it shows that run's composer), else the main window. Every window
// listens (installQuoteAppend, from main.ts); only the named one writes.
//
// FE-only: the event goes through `plugin:event|emit`, which the remote bridge
// refuses (src-tauri/src/remote/handler.rs `ipc_refuses`), so a controller can't
// post one here.
import { WebviewWindow, getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emit, listen } from '@tauri-apps/api/event'
import { fileQuote, type QuoteSource } from '@/lib/quote'
import { popoutLabel } from '@/lib/popout'
import { sessionWindowLabel } from '@/lib/sessionWindow'
import { useChatDraftsStore } from '@/stores/chatDrafts'

export const QUOTE_APPEND = 'quote:append'

export interface QuoteAppend {
  runId: string
  text: string
  source: QuoteSource
  /** Typed in the file window's Quote popover (UX-46). */
  note?: string
  /** Label of the one window that writes it. */
  writer: string
}

/** A composer on screen: takes the quotes for the run it shows. */
export interface QuoteComposer {
  runId: () => string | null
  append: (text: string, source: QuoteSource, note?: string) => void
}

const composers = new Set<QuoteComposer>()

/** Route quotes for the composer's run into it while it lives. Returns the unregister. */
export function registerQuoteComposer(composer: QuoteComposer): () => void {
  composers.add(composer)
  return () => {
    composers.delete(composer)
  }
}

/** Add a file window's quote to a run's stored draft (no composer is showing it), as a card. */
export function appendQuoteToDraft(runId: string, text: string, source: QuoteSource, note?: string) {
  if (!text.trim()) return
  const drafts = useChatDraftsStore()
  const draft = drafts.get(runId)
  drafts.set(runId, { ...draft, quotes: [...(draft.quotes ?? []), fileQuote(crypto.randomUUID(), text, source, note)] })
}

/**
 * The one window that writes a run's composer state: its session pop-out when
 * open, else the main window. The message queue (UX-25) sends from it too.
 */
export async function composerWindowLabel(runId: string): Promise<string> {
  const session = sessionWindowLabel(runId)
  return (await WebviewWindow.getByLabel(session)) ? session : popoutLabel('main')
}

export async function sendQuote(runId: string, text: string, source: QuoteSource, note?: string): Promise<void> {
  const writer = await composerWindowLabel(runId)
  await emit(QUOTE_APPEND, { runId, text, source, note, writer } satisfies QuoteAppend)
}

/** Listen for quotes this window is the writer of. Once per webview, after Pinia. */
export async function installQuoteAppend(): Promise<void> {
  try {
    const me = getCurrentWebviewWindow().label
    await listen<QuoteAppend>(QUOTE_APPEND, ({ payload: q }) => {
      if (q?.writer !== me || !q.runId) return
      const composer = [...composers].find((c) => c.runId() === q.runId)
      if (composer) composer.append(q.text, q.source, q.note)
      else appendQuoteToDraft(q.runId, q.text, q.source, q.note)
    })
  } catch {
    /* running outside the Tauri shell */
  }
}
