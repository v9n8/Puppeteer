import { defineStore } from 'pinia'
import { invoke } from '@/lib/tauri'
import { ref } from 'vue'
import type { AccountBudget } from '@/stores/claudeAccounts'

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
  // Per-account plan-usage verdict, keyed by account id (UX-41).
  const budgets = ref<Record<string, AccountBudget>>({})
  // Per-account live-probe state for the usage ring's refresh button.
  const refreshingUsage = ref<Record<string, boolean>>({})
  const usageErrors = ref<Record<string, string | null>>({})

  async function fetch() {
    loading.value = true
    error.value = null
    try {
      accounts.value = await invoke<CodexAccount[]>('list_codex_accounts')
      await Promise.all(accounts.value.map(a => fetchBudget(a.id)))
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function fetchBudget(id: string) {
    try {
      budgets.value[id] = await invoke<AccountBudget>('get_codex_budget_status', { accountId: id })
    } catch {
      // Best-effort; leave any prior value.
    }
  }

  // Probe Codex for one account's rate limits (under its CODEX_HOME), then
  // re-read its verdict.
  async function refreshUsage(id: string): Promise<void> {
    if (refreshingUsage.value[id]) return
    refreshingUsage.value[id] = true
    usageErrors.value[id] = null
    try {
      await invoke('refresh_codex_plan_usage', { accountId: id })
    } catch (e) {
      usageErrors.value[id] = String(e)
      // Probe is best-effort; fall back to the latest stored snapshot below.
    } finally {
      refreshingUsage.value[id] = false
      await fetchBudget(id)
    }
  }

  // Every account once per app session (the startup probe covers only the
  // shared ~/.codex login), one at a time. See the Claude store.
  let usageProbed = false
  async function probeAccountsUsageOnce(): Promise<void> {
    if (usageProbed) return
    usageProbed = true
    for (const a of accounts.value) await refreshUsage(a.id)
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

  return {
    accounts, loading, error, budgets, refreshingUsage, usageErrors,
    fetch, fetchBudget, refreshUsage, probeAccountsUsageOnce, create, rename, remove, setDefault, login, cancelLogin, validate,
  }
})
