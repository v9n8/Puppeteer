import { defineStore } from 'pinia'
import { invoke } from '@/lib/tauri'
import { ref } from 'vue'

// Codex (ChatGPT) accounts, each signed in with `codex login` under its own
// CODEX_HOME. No account is the default until the user picks one: Codex runs
// then use the shared `~/.codex` login.
export interface CodexAccount {
  id: string
  label: string
  home_dir: string
  email: string | null
  auth_method: string | null
  status: string
  is_default: boolean
  last_checked_at: string | null
  created_at: string
}

export interface CodexValidation {
  logged_in: boolean
  status: string
  email: string | null
  auth_method: string | null
}

export const useCodexAccountsStore = defineStore('codexAccounts', () => {
  const accounts = ref<CodexAccount[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function fetch() {
    loading.value = true
    error.value = null
    try {
      accounts.value = await invoke<CodexAccount[]>('list_codex_accounts')
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function create(label: string): Promise<CodexAccount> {
    const account = await invoke<CodexAccount>('create_codex_account', { label })
    await fetch()
    return account
  }

  async function rename(id: string, label: string): Promise<CodexAccount> {
    const account = await invoke<CodexAccount>('rename_codex_account', { id, label })
    await fetch()
    return account
  }

  async function remove(id: string): Promise<void> {
    await invoke('delete_codex_account', { id })
    await fetch()
  }

  /** `null` clears the default: Codex runs go back to the shared `~/.codex` login. */
  async function setDefault(id: string | null): Promise<void> {
    await invoke('set_default_codex_account', { id })
    await fetch()
  }

  // Runs `codex login` (or `codex login --device-auth`) in the background;
  // resolves once the CLI exits (or rejects on cancel/timeout/failure).
  async function login(id: string, deviceAuth = false): Promise<CodexValidation> {
    try {
      return await invoke<CodexValidation>('login_codex_account', { id, deviceAuth })
    } finally {
      await fetch()
    }
  }

  async function cancelLogin(id: string): Promise<void> {
    await invoke('cancel_codex_account_login', { id })
  }

  async function validate(id: string): Promise<CodexValidation> {
    const result = await invoke<CodexValidation>('validate_codex_account', { id })
    await fetch()
    return result
  }

  return { accounts, loading, error, fetch, create, rename, remove, setDefault, login, cancelLogin, validate }
})
