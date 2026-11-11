// Settings → Engines & accounts (UX-42): the one list of Claude and Codex
// accounts, and the steps of a sign-in. Pure, so it is unit-tested
// (__tests__/engineAccounts.test.mjs); the components only add the labels.

export type AccountKind = 'claude' | 'codex'

export function accountKey(kind: AccountKind, id: string): string {
  return `${kind}:${id}`
}

/** Row key of the shared `~/.codex` login. */
export const SHARED_CODEX_KEY = 'codex:shared'

interface AccountLike {
  id: string
  label: string
  email: string | null
  auth_method: string | null
  status: string
  is_default: boolean
}

/** A budget verdict; only a `plan` one carries a real percentage. */
interface PlanVerdict {
  source: string
  percent: number
  period: string
  is_warning: boolean
  is_over: boolean
}

export interface AccountUsage {
  percent: number
  period: string
  warning: boolean
  over: boolean
}

export interface AccountRow {
  key: string
  kind: AccountKind
  /** null: the shared ~/.codex login, which is not an account of Puppeteer's. */
  id: string | null
  label: string
  status: string | null
  /** E-mail, else the sign-in method; the folder for the shared login. */
  detail: string | null
  isDefault: boolean
  usage: AccountUsage | null
}

function usageOf(v: PlanVerdict | null | undefined): AccountUsage | null {
  return v?.source === 'plan'
    ? { percent: v.percent, period: v.period, warning: v.is_warning, over: v.is_over }
    : null
}

function row(kind: AccountKind, a: AccountLike, usage: AccountUsage | null): AccountRow {
  return {
    key: accountKey(kind, a.id),
    kind,
    id: a.id,
    label: a.label,
    status: a.status,
    detail: a.email ?? a.auth_method,
    isDefault: a.is_default,
    usage,
  }
}

/**
 * Claude accounts, then Codex. Each kind has its own default. For Codex that is
 * the shared `~/.codex` login until an account is picked, so once a Codex
 * account exists that login gets a row of its own (first among Codex) to pick
 * it back. Every login has its own usage.
 */
export function accountRows(input: {
  claude: AccountLike[]
  claudeBudgets: Record<string, PlanVerdict | undefined>
  codex: AccountLike[]
  codexBudgets: Record<string, PlanVerdict | undefined>
  sharedCodexLabel: string
  sharedCodexUsage: PlanVerdict | null
}): AccountRow[] {
  const rows = input.claude.map((a) => row('claude', a, usageOf(input.claudeBudgets[a.id])))
  if (input.codex.length) {
    rows.push({
      key: SHARED_CODEX_KEY,
      kind: 'codex',
      id: null,
      label: input.sharedCodexLabel,
      status: null,
      detail: '~/.codex',
      isDefault: !input.codex.some((a) => a.is_default),
      usage: usageOf(input.sharedCodexUsage),
    })
    rows.push(...input.codex.map((a) => row('codex', a, usageOf(input.codexBudgets[a.id]))))
  }
  return rows
}

export type LoginStepState = 'done' | 'active' | 'pending'

/**
 * The steps of a running sign-in. `open`: the sign-in page, done once the CLI
 * printed its link. `code` (Codex device flow only): the one-time code to type
 * on that page. `wait`: the CLI waiting for the sign-in, which in the device
 * flow can't start before the code is there.
 */
export function loginSteps(run: { url: string | null; code: string | null; deviceAuth: boolean }) {
  const steps: { key: 'open' | 'code' | 'wait'; state: LoginStepState }[] = [
    { key: 'open', state: run.url ? 'done' : 'active' },
  ]
  if (run.deviceAuth) steps.push({ key: 'code', state: run.code ? 'done' : 'active' })
  steps.push({ key: 'wait', state: run.deviceAuth && !run.code ? 'pending' : 'active' })
  return steps
}
