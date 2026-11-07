// Feature toggles: the backend contract (src-tauri/src/features) plus the pure
// checks the sidebar, menus, router and pop-out windows share. Kept free of
// Vue / Tauri / `@/` imports so the helpers can be unit-tested with `node --test`.
//
// The backend resolves the tree (defaults, locks, module → prop); this side only
// reads it. Ids are matched whole, never split on `.`: a prop id may itself
// contain dots (`text_tools.selection.translate`).

export const CMD_GET_FEATURES = 'get_features'
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
  return items.filter((item) => item.feature === undefined || isFeatureOn(tree, item.feature))
}
