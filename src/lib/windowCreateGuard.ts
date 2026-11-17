// Shared by every lib/*Window.ts: waits for the new window and, when creation fails,
// tells the user and throws — detachScreen relies on the throw to put the screen back.
import { whenWindowCreated } from '@/lib/windowCreated'
import { useToast } from '@/composables/useToast'
import { i18n } from '@/i18n'

const reported = new WeakSet<object>()

/** True when awaitWindowCreated already toasted this error (callers must not toast again). */
export function isWindowErrorReported(e: unknown): boolean {
  return typeof e === 'object' && e !== null && reported.has(e)
}

export async function awaitWindowCreated(
  win: Parameters<typeof whenWindowCreated>[0],
  tag: string,
): Promise<void> {
  try {
    await whenWindowCreated(win)
  } catch (e) {
    console.error(`[${tag}] failed to open window`, e)
    useToast().toast.error(i18n.global.t('nav.windowOpenFailed', { error: String(e) }))
    if (typeof e === 'object' && e !== null) reported.add(e)
    throw e
  }
}
