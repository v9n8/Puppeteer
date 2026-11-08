// Feature toggles: the backend contract (src-tauri/src/features) plus the pure
// checks the sidebar, menus, router and pop-out windows share. Kept free of
// Vue / Tauri / `@/` imports so the helpers can be unit-tested with `node --test`.
//
// The backend resolves the tree (defaults, locks, module → prop); this side only
// reads it. Ids are matched whole, never split on `.`: a prop id may itself
// contain dots (`text_tools.selection.translate`).

export const CMD_GET_FEATURES = 'get_features'
/** `{ id, enabled }` → the new tree. Local-only. */
export const CMD_SET_FEATURE = 'set_feature'
/** `{ dryRun }` → ids it turns off; a dry run writes nothing. Local-only. */
export const CMD_RESET_FEATURES = 'reset_features'
/** Broadcast after every write; the payload is the whole new tree. */
export const EVT_FEATURES_CHANGED = 'features:changed'

export type FeatureReasonKey =
  | 'features.reason.locked'
  | 'features.reason.module_off'
  | 'features.reason.missing_dependency'

export interface PropState {
  /** Full id, `module.prop`. */
  id: string
  /** Saved value; `null` = never saved (the default applies). */
  stored: boolean | null
  effective: boolean
  locked: boolean
  default: boolean
  /** Full ids of dependencies that are off, while this prop is on. */
  degraded: string[]
  /** i18n key explaining a locked, module-off or degraded prop. */
  reason_key: FeatureReasonKey | null
}

export interface ModuleState {
  module: string
  enabled: boolean
  locked: boolean
  props: PropState[]
}

export type FeatureTree = ModuleState[]

/**
 * Whether a module (`productivity`) or prop (`productivity.focus`) is on.
 * A prop inside a module that is off is already off in the tree; a prop with a
 * missing dependency stays on (degraded). An id the tree does not know is off.
 */
export function isFeatureOn(tree: FeatureTree, id: string): boolean {
  for (const m of tree) {
    if (m.module === id) return m.enabled
    const prop = m.props.find((p) => p.id === id)
    if (prop) return prop.effective
  }
  return false
}

/** Items without a `feature`, plus those whose feature is on (order kept). */
export function filterByFeature<T extends { feature?: string }>(items: T[], tree: FeatureTree): T[] {
  return items.filter((item) => allowsFeature(tree, item.feature))
}

/** Ungated (`feature` unset) or on. */
export function allowsFeature(tree: FeatureTree, feature: string | undefined): boolean {
  return feature === undefined || isFeatureOn(tree, feature)
}

/**
 * True when `items[index]` starts a new group (and so gets a divider). Run on
 * the filtered list, so a hidden entry never leaves a doubled or dangling one.
 */
export function startsGroup(items: readonly { group: string }[], index: number): boolean {
  return index > 0 && items[index].group !== items[index - 1].group
}

interface GatedRoute {
  path: string
  feature?: string
}

/** The feature a path needs: that of the gated route it is, or lies under. */
export function routeFeature(path: string, routes: readonly GatedRoute[]): string | undefined {
  return routes.find((r) => r.feature && (path === r.path || path.startsWith(`${r.path}/`)))?.feature
}

/**
 * App-menu actions (lib/appMenu, also run by the tray's quick actions) that open
 * a gated window. `go.<path>` actions follow their route instead.
 */
export const MENU_ACTION_FEATURES: Readonly<Record<string, string>> = {
  'file.newTodo': 'productivity.todos',
  'file.newNote': 'productivity.notes',
  'window.todoList': 'productivity.todos',
  'window.noteList': 'productivity.notes',
  'window.secretNotes': 'security.secret_vault',
  'window.slackThreads': 'integrations.slack',
}

/** The feature an app-menu action needs, if any. */
export function menuActionFeature(id: string, routes: readonly GatedRoute[]): string | undefined {
  if (id.startsWith('go.')) return routes.find((r) => r.path === id.slice(3))?.feature
  return MENU_ACTION_FEATURES[id]
}

// Query flag of each gated pop-out kind (App.vue) → its feature.
const POPOUT_FEATURES: readonly (readonly [string, string])[] = [
  ['focusWindow', 'productivity.focus'],
  ['prInboxWindow', 'code_hosting.pr_inbox'],
  ['calendarWindow', 'productivity.calendar'],
  ['ganttWindow', 'code_hosting.gantt'],
  ['secretNotesWindow', 'security.secret_vault'],
  ['slackWindow', 'integrations.slack'],
]

/**
 * The feature a pop-out window needs, from its URL query (`?focusWindow=1`).
 * The list window serves both todos and notes, so its `kind` decides.
 */
export function popoutFeature(search: string): string | undefined {
  const q = new URLSearchParams(search)
  if (q.get('itemListWindow') === '1') {
    return q.get('kind') === 'note' ? 'productivity.notes' : 'productivity.todos'
  }
  return POPOUT_FEATURES.find(([flag]) => q.get(flag) === '1')?.[1]
}

export interface PropRow {
  id: string
  /** The prop's own value — what it is back to once its module is on again. */
  checked: boolean
  /** Can be flipped here: not locked, its module on, not read-only. */
  editable: boolean
  /** Shown dimmed: locked, or its module is off. */
  muted: boolean
  reasonKey: FeatureReasonKey | null
  /** Full ids of the dependencies that are off (prop runs degraded). */
  degraded: string[]
}

export interface ModuleRow {
  module: string
  checked: boolean
  editable: boolean
  locked: boolean
  /** Props in effect, for the collapsed row. */
  onCount: number
  props: PropRow[]
}

/**
 * Rows of Settings → Features. A prop under a module that is off shows its own
 * stored value (turning the module back on restores it), not `effective`.
 * `readOnly`: a remote-desktop window, where every switch is local-only.
 */
export function featureRows(tree: FeatureTree, readOnly: boolean): ModuleRow[] {
  return tree.map((m) => ({
    module: m.module,
    checked: m.enabled,
    editable: !readOnly && !m.locked,
    locked: m.locked,
    onCount: m.props.filter((p) => p.effective).length,
    props: m.props.map((p) => ({
      id: p.id,
      checked: p.locked || (p.stored ?? p.default),
      editable: !readOnly && !p.locked && m.enabled,
      muted: p.locked || !m.enabled,
      reasonKey: p.reason_key,
      degraded: p.degraded,
    })),
  }))
}

/**
 * Entry points outside the sidebar and menus — a button, tab, chip or panel —
 * and every feature each needs on. Kept in one table so a test can check every
 * id against the backend registry.
 */
export const ENTRY_FEATURES = {
  'run.linkedSlack': ['integrations.slack'],
  'run.linkedTodos': ['productivity.todos'],
  'run.linkedNotes': ['productivity.notes'],
  'run.conductorPanel': ['orchestration.conductor'],
  'run.handoff': ['sessions.handoff'],
  'todo.add': ['productivity.todos'],
  'projects.gantt': ['code_hosting.gantt'],
  'item.todo': ['productivity.todos'],
  'item.note': ['productivity.notes'],
  'capture.slack': ['integrations.slack'],
  'capture.note': ['productivity.notes'],
  'capture.todo': ['productivity.todos'],
  'library.manageGroups': ['library.groups'],
  // Busy time and pushing a plan both go through the Google account.
  'focus.calendar': ['productivity.calendar', 'integrations.google'],
} as const satisfies Record<string, readonly string[]>

export type GatedEntry = keyof typeof ENTRY_FEATURES

/** Every feature `entry` needs is on. */
export function entryOn(tree: FeatureTree, entry: GatedEntry): boolean {
  return ENTRY_FEATURES[entry].every((id) => isFeatureOn(tree, id))
}

/**
 * The tab (or filter) to show: `current` while it is still offered, else the
 * first one offered, `null` when none is.
 */
export function pickAvailable<K extends string>(current: K, offered: readonly K[]): K | null {
  return offered.includes(current) ? current : (offered[0] ?? null)
}

/** Where every turned-away destination lands (router guard, App.vue). */
export const FALLBACK_PATH = '/projects'

/**
 * Where to go when the feature of the page on screen is off (switched off while
 * it is open): {@link FALLBACK_PATH}, or `null` to stay.
 */
export function fallbackPath(path: string, routes: readonly GatedRoute[], tree: FeatureTree): string | null {
  return allowsFeature(tree, routeFeature(path, routes)) ? null : FALLBACK_PATH
}

type Translate = (key: string, params?: Record<string, unknown>) => string

/** i18n key of a module (`productivity`) or prop (`productivity.focus`) name. */
export function featureLabelKey(id: string): string {
  // Module ids have no dot; every prop id is `module.prop`.
  return id.includes('.') ? `features.prop.${id}` : `features.module.${id}`
}

/**
 * A readable message for the backend's feature refusals — `feature_disabled:
 * <id>[, <id>]` and `remote_denied_local_only` — or `null` for any other error.
 */
export function featureErrorMessage(err: unknown, t: Translate): string | null {
  const msg = String(err)
  if (msg.includes('remote_denied_local_only')) return t('features.error.localOnly')
  const ids = msg.match(/feature_disabled: ([\w.]+(?:, [\w.]+)*)/)?.[1].split(', ')
  if (!ids) return null
  return t('features.error.disabled', { name: ids.map((id) => t(featureLabelKey(id))).join(', ') })
}

/** {@link featureErrorMessage}, else the error as it came. */
export function errorText(err: unknown, t: Translate): string {
  return featureErrorMessage(err, t) ?? String(err)
}
