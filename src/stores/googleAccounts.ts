import { defineStore } from 'pinia'
import { invoke } from '@/lib/tauri'
import { ref } from 'vue'
import type { GoogleAccount } from '@/stores/googleCalendar'

// Google accounts (Drive / Gmail / Calendar via OAuth) as Settings → Connections
// lists them: shared by the list, its nav badge and the Google editor, so an
// add or delete in the editor shows in the list at once. The Calendar keeps its
// own copy (googleCalendar), with its per-account visibility.
export const useGoogleAccountsStore = defineStore('googleAccounts', () => {
  const accounts = ref<GoogleAccount[]>([])

  async function fetch() {
    accounts.value = await invoke<GoogleAccount[]>('list_google_accounts')
  }

  return { accounts, fetch }
})
