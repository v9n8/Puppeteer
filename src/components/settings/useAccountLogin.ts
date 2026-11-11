// Sign-ins of the Settings accounts (Claude and Codex), shared by the account
// list ("Sign in again") and the "Add account" dialog. Each one runs the
// engine's own CLI in the background (`claude auth login`, `codex login`) and
// settles when it exits; the page link it prints and Codex's device code
// arrive as events. Create it once per screen: it cancels whatever is still
// running when that screen goes away.
import { onMounted, onUnmounted, reactive } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { accountKey, type AccountKind } from '@/lib/engineAccounts'

export interface LoginRun {
  state: 'running' | 'done' | 'failed' | 'cancelled'
  /** The sign-in page the CLI printed: Claude's manual page, Codex's fallback or device page. */
  url: string | null
  /** Codex `--device-auth`: the one-time code to enter on `url`. */
  code: string | null
  deviceAuth: boolean
  email: string | null
  error: string | null
}

export function useAccountLogin() {
  const { t } = useI18n()
  const claudeStore = useClaudeAccountsStore()
  const codexStore = useCodexAccountsStore()

  const runs = reactive<Record<string, LoginRun>>({})
  const pending = new Map<string, Promise<boolean>>()
  const unlisten: UnlistenFn[] = []

  function running(kind: AccountKind, id: string): LoginRun | null {
    const run = runs[accountKey(kind, id)]
    return run?.state === 'running' ? run : null
  }

  /** Resolves with whether the account ended up signed in; never rejects. */
  async function start(kind: AccountKind, id: string, deviceAuth = false): Promise<boolean> {
    const key = accountKey(kind, id)
    const prev = pending.get(key)
    if (prev) {
      if (runs[key]?.deviceAuth === deviceAuth) return prev
      // Switching flow (Codex browser → device code): the CLI allows one login
      // per account, so the old one has to end first.
      await stop(kind, id)
    }
    const run = reactive<LoginRun>({ state: 'running', url: null, code: null, deviceAuth, email: null, error: null })
    runs[key] = run
    const done = (async () => {
      try {
        const result = kind === 'claude' ? await claudeStore.login(id) : await codexStore.login(id, deviceAuth)
        run.email = result.email
        run.state = result.logged_in ? 'done' : 'failed'
        if (!result.logged_in) run.error = t('settings.accounts.login.notSignedIn')
        return result.logged_in
      } catch (e) {
        const msg = String(e)
        if (msg.includes('cancelled')) {
          run.state = 'cancelled'
        } else {
          run.state = 'failed'
          run.error = msg
        }
        return false
      } finally {
        if (runs[key] === run) pending.delete(key)
      }
    })()
    pending.set(key, done)
    return done
  }

  function cancel(kind: AccountKind, id: string): Promise<void> {
    const store = kind === 'claude' ? claudeStore : codexStore
    return store.cancelLogin(id).catch(() => {})
  }

  /** Cancel and wait until the CLI is gone (before deleting the account, or a restart). */
  async function stop(kind: AccountKind, id: string): Promise<void> {
    const p = pending.get(accountKey(kind, id))
    if (!p) return
    await cancel(kind, id)
    await p
  }

  /** Claude only: the code the manual sign-in page shows, for the waiting CLI. */
  async function submitCode(id: string, code: string): Promise<void> {
    const run = running('claude', id)
    if (!run || !code.trim()) return
    run.error = null
    try {
      await claudeStore.submitLoginCode(id, code.trim())
    } catch (e) {
      run.error = String(e)
    }
  }

  function clear(kind: AccountKind, id: string) {
    const key = accountKey(kind, id)
    if (runs[key]?.state !== 'running') delete runs[key]
  }

  onMounted(async () => {
    unlisten.push(
      await listen<{ accountId: string; url: string }>('claude_login_url', (e) => {
        const run = running('claude', e.payload.accountId)
        if (run) run.url = e.payload.url
      }),
      await listen<{ accountId: string; url: string }>('codex_login_url', (e) => {
        const run = running('codex', e.payload.accountId)
        if (run) run.url = e.payload.url
      }),
      await listen<{ accountId: string; code: string }>('codex_login_code', (e) => {
        const run = running('codex', e.payload.accountId)
        if (run) run.code = e.payload.code
      }),
    )
  })

  onUnmounted(() => {
    for (const fn of unlisten) fn()
    // Don't leave a background login behind a screen that's gone.
    for (const [key, run] of Object.entries(runs)) {
      if (run.state !== 'running') continue
      const i = key.indexOf(':')
      void cancel(key.slice(0, i) as AccountKind, key.slice(i + 1))
    }
  })

  return { runs, running, start, cancel, stop, submitCode, clear }
}

export type AccountLogin = ReturnType<typeof useAccountLogin>
