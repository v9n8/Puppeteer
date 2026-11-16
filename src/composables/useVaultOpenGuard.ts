// "The secret notes vault is open in this window." Set by the Secret notes screen
// while it is the `/secret-notes` screen of the main window and unlocked; read by
// App.vue to keep the selection toolbar (Quote, Translate, Copy MD, Note, Todo)
// off, because the main window's webview also renders Session text and other
// pages, and a secret must never reach Translate or the clipboard shortcuts.
// Module state: each webview has its own copy.
import { readonly, ref } from 'vue'

const vaultOpenHere = ref(false)

export function setVaultOpenHere(open: boolean): void {
  vaultOpenHere.value = open
}

export function useVaultOpenGuard() {
  return { vaultOpenHere: readonly(vaultOpenHere) }
}
