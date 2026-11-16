// ⌘S (Ctrl+S on Windows / Linux) saves a file while it is being edited.
// `el` is any element of the editor's pane: a file tab stays mounted while it is
// hidden, so only the pane that is on screen answers the key.
import { onMounted, onUnmounted, type Ref } from 'vue'
import { desktopOs } from '@/lib/terminalApps'

export function useSaveShortcut(editing: Ref<boolean>, save: () => void, el: Ref<HTMLElement | null>) {
  const mac = desktopOs(navigator.userAgent) === 'macos'

  function onKeydown(e: KeyboardEvent) {
    if (e.key.toLowerCase() !== 's' || e.altKey || e.shiftKey) return
    if (mac ? !e.metaKey || e.ctrlKey : !e.ctrlKey || e.metaKey) return
    if (!editing.value || !el.value || el.value.offsetParent === null) return
    // Keeps the webview from opening its own "save page" dialog.
    e.preventDefault()
    save()
  }

  onMounted(() => window.addEventListener('keydown', onKeydown))
  onUnmounted(() => window.removeEventListener('keydown', onKeydown))

  return { shortcutLabel: mac ? '⌘S' : 'Ctrl+S' }
}
