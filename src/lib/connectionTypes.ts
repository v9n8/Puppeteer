import { Cloud, CloudCog, Github, Gitlab, HardDrive } from 'lucide-vue-next'
import { filterByFeature, type FeatureTree } from './featureGate.ts'

// Settings → Connections: one list for the accounts Puppeteer runs CLIs and
// tools as. Each type keeps its own table, Tauri commands and store; this
// registry only describes them, so the list, its filter, the "Add connection"
// picker, the row's Validate button and the nav badge are built the same way
// for every type. Kept free of Vue / Tauri / `@/` imports for `node --test`.
//
// A type is listed only once it has a real validate command and at least one
// real consumer (the __tests__ check both against the code). Adding, editing
// and deleting stay in each type's own editor (ConnectionsSettings.vue maps
// the type to it).

/** Where an account of a type is used. Shown as "Used by" on the account. */
export const CONNECTION_CONSUMERS = [
  'projects',
  'sessionChips',
  'cliBroker',
  'prInbox',
  'issueLinks',
  'mcpGitAccounts',
  'mcpCloudAccounts',
  'googleMcp',
  'calendar',
] as const
export type ConnectionConsumer = (typeof CONNECTION_CONSUMERS)[number]

/** i18n key of a consumer's name. */
export function consumerLabelKey(c: ConnectionConsumer): string {
  return `settings.connections.consumers.${c}`
}

/** The fields of an account the list reads; every store's account type fits it. */
export interface AccountLike {
  id: string
  label: string
  username?: string | null
  host?: string | null
  email?: string | null
  profile_name?: string | null
  region?: string
  account_id?: string | null
  subscription_name?: string | null
  subscription_id?: string | null
  user_name?: string | null
  last_validated_at?: string | null
  logged_in?: boolean
  is_default?: boolean
}

/**
 * - `validated`: the last check passed (`validatedAt` when the type stores it);
 * - `unvalidated`: never checked;
 * - `needsLogin`: the stored sign-in expired;
 * - `connected`: signed in through OAuth, no check result is stored.
 */
export type ConnectionStatus = 'validated' | 'unvalidated' | 'needsLogin' | 'connected'

/** The project field listing the accounts of a type a project enabled. */
export type ProjectAccountField =
  | 'github_account_ids'
  | 'gitlab_account_ids'
  | 'aws_account_ids'
  | 'azure_account_ids'

export type ProjectLike = Partial<Record<ProjectAccountField, readonly string[]>>

/** What a passing validate call may still warn about (e.g. a missing scope). */
export interface ValidateNote {
  outcome: 'warn' | 'needsLogin'
  messageKey: string
}

export interface ConnectionType {
  id: string
  labelKey: string
  icon: typeof Github
  /** Hidden from the list, the picker and the badge while off (lib/featureGate). */
  feature?: string
  /** Tauri command checking one account; every type's takes `{ id }`. */
  validateCommand: string
  /** Text in a validate error meaning "sign in again" rather than a failure. */
  loginRequiredMarker?: string
  /** A passing validate result that still needs attention. */
  validateNote?: (result: unknown) => ValidateNote | null
  /** i18n keys of the secrets Puppeteer keeps for the account in the OS keychain. */
  secretKeys: readonly string[]
  consumers: readonly ConnectionConsumer[]
  /** Counts the projects using an account; none for types not bound to projects. */
  projectField?: ProjectAccountField
  /** Second line of the row. */
  detail: (a: AccountLike) => string
  status: (a: AccountLike) => ConnectionStatus
}

const join = (parts: (string | null | undefined)[]) => parts.filter(Boolean).join(' · ')
const at = (s: string | null | undefined) => (s ? `@${s}` : null)

export const CONNECTION_TYPES = [
  {
    id: 'github',
    labelKey: 'settings.connections.types.github',
    icon: Github,
    validateCommand: 'validate_github_account',
    validateNote: (r) => ((r as { has_repo_scope?: boolean })?.has_repo_scope === false
      ? { outcome: 'warn', messageKey: 'settings.github.missingRepoScope' }
      : null),
    secretKeys: ['settings.connections.secrets.pat'],
    consumers: ['projects', 'sessionChips', 'cliBroker', 'prInbox', 'issueLinks', 'mcpGitAccounts'],
    projectField: 'github_account_ids',
    detail: (a) => join([at(a.username)]),
    // Validating saves the username: an account without one was never checked.
    status: (a) => (a.username ? 'validated' : 'unvalidated'),
  },
  {
    id: 'gitlab',
    labelKey: 'settings.connections.types.gitlab',
    icon: Gitlab,
    validateCommand: 'validate_gitlab_account',
    secretKeys: ['settings.connections.secrets.pat'],
    consumers: ['projects', 'sessionChips', 'cliBroker', 'issueLinks', 'mcpGitAccounts'],
    projectField: 'gitlab_account_ids',
    detail: (a) => join([at(a.username), a.host]),
    status: (a) => (a.username ? 'validated' : 'unvalidated'),
  },
  {
    id: 'aws',
    labelKey: 'settings.connections.types.aws',
    icon: Cloud,
    feature: 'infra_access.aws',
    validateCommand: 'validate_aws_account',
    loginRequiredMarker: 'SSO_LOGIN_REQUIRED',
    // Profile / SSO only: the credentials stay in ~/.aws.
    secretKeys: [],
    consumers: ['projects', 'sessionChips', 'cliBroker', 'mcpCloudAccounts'],
    projectField: 'aws_account_ids',
    detail: (a) => join([a.profile_name, a.region, a.account_id]),
    status: (a) => (a.last_validated_at ? 'validated' : 'unvalidated'),
  },
  {
    id: 'azure',
    labelKey: 'settings.connections.types.azure',
    icon: CloudCog,
    feature: 'infra_access.azure',
    validateCommand: 'validate_azure_account',
    loginRequiredMarker: 'AZURE_LOGIN_REQUIRED',
    // Each account is its own `az login` config dir, not a keychain entry.
    secretKeys: [],
    consumers: ['projects', 'sessionChips', 'cliBroker', 'mcpCloudAccounts'],
    projectField: 'azure_account_ids',
    detail: (a) => join([a.subscription_name ?? a.subscription_id, a.user_name]),
    status: (a) => (a.logged_in === false ? 'needsLogin' : a.last_validated_at ? 'validated' : 'unvalidated'),
  },
  {
    id: 'google',
    labelKey: 'settings.connections.types.google',
    icon: HardDrive,
    feature: 'integrations.google',
    validateCommand: 'validate_google_account',
    loginRequiredMarker: 'GOOGLE_LOGIN_REQUIRED',
    // Missing scopes are fixed by signing in again (the editor shows Sign in).
    validateNote: (r) => ((r as { missing_scopes?: string[] })?.missing_scopes?.length
      ? { outcome: 'needsLogin', messageKey: 'settings.connections.check.missingScopes' }
      : null),
    secretKeys: ['settings.connections.secrets.oauthToken', 'settings.connections.secrets.clientSecret'],
    consumers: ['googleMcp', 'calendar'],
    detail: (a) => join([a.email]),
    status: () => 'connected',
  },
] as const satisfies readonly ConnectionType[]

export type ConnectionTypeId = (typeof CONNECTION_TYPES)[number]['id']
export type ConnectionFilter = ConnectionTypeId | 'all'

/** Accounts of each type, as their stores hold them. */
export type ConnectionSources = Partial<Record<ConnectionTypeId, readonly AccountLike[]>>

export function isConnectionTypeId(id: unknown): id is ConnectionTypeId {
  return CONNECTION_TYPES.some((t) => t.id === id)
}

export function connectionType(id: ConnectionTypeId): ConnectionType {
  return CONNECTION_TYPES.find((t) => t.id === id)!
}

/** Types whose feature is on, in registry order. */
export function enabledConnectionTypes(tree: FeatureTree): ConnectionType[] {
  return filterByFeature<ConnectionType>([...CONNECTION_TYPES], tree)
}

export interface ConnectionRow {
  type: ConnectionTypeId
  id: string
  label: string
  detail: string
  status: ConnectionStatus
  validatedAt: string | null
  isDefault: boolean
  /** Projects that enabled the account; null for types not bound to projects. */
  projectCount: number | null
}

/** One row per account of every given type, by type then store order. */
export function buildConnectionRows(
  types: readonly ConnectionType[],
  sources: ConnectionSources,
  projects: readonly ProjectLike[],
): ConnectionRow[] {
  return types.flatMap((def) => (sources[def.id as ConnectionTypeId] ?? []).map((a) => ({
    type: def.id as ConnectionTypeId,
    id: a.id,
    label: a.label,
    detail: def.detail(a),
    status: def.status(a),
    validatedAt: a.last_validated_at ?? null,
    isDefault: !!a.is_default,
    projectCount: def.projectField
      ? projects.filter((p) => p[def.projectField!]?.includes(a.id)).length
      : null,
  })))
}

export function filterConnectionRows(rows: readonly ConnectionRow[], filter: ConnectionFilter): ConnectionRow[] {
  return filter === 'all' ? [...rows] : rows.filter((r) => r.type === filter)
}

/** Accounts of the given (enabled) types: the nav badge. */
export function countConnections(types: readonly ConnectionType[], sources: ConnectionSources): number {
  return types.reduce((n, def) => n + (sources[def.id as ConnectionTypeId]?.length ?? 0), 0)
}

/** A filter left pointing at a type that got switched off falls back to all. */
export function effectiveFilter(filter: ConnectionFilter, types: readonly ConnectionType[]): ConnectionFilter {
  return filter === 'all' || types.some((t) => t.id === filter) ? filter : 'all'
}

export type CheckResult =
  | { outcome: 'ok' }
  | { outcome: 'warn' | 'needsLogin'; messageKey: string }
  | { outcome: 'error'; message: string }

/** Turns a validate call's result (or error) into what the row shows. */
export function checkResult(def: ConnectionType, result: { ok: unknown } | { error: unknown }): CheckResult {
  if ('error' in result) {
    const message = String(result.error)
    if (def.loginRequiredMarker && message.includes(def.loginRequiredMarker)) {
      return { outcome: 'needsLogin', messageKey: 'settings.connections.status.needsLogin' }
    }
    return { outcome: 'error', message }
  }
  return def.validateNote?.(result.ok) ?? { outcome: 'ok' }
}
