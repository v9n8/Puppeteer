// The menu bar's actions grouped by their top-level menu, for Settings → Keyboard
// shortcuts. Pure (no store, no Tauri) so `node --test` can run it; appMenu.ts
// feeds it the built spec.

export interface MenuAction {
  id: string
  label: string
}

export interface MenuActionGroup {
  /** The top-level menu's label (File, Edit, …). */
  menu: string
  actions: MenuAction[]
}

type MenuItem =
  | { kind: 'item'; id: string; label: string }
  | { kind: 'submenu'; items: readonly MenuItem[] }
  | { kind: string }

function flatten(items: readonly MenuItem[]): MenuAction[] {
  return items.flatMap((item): MenuAction[] => {
    if (item.kind === 'item') {
      const { id, label } = item as { id: string; label: string }
      return [{ id, label }]
    }
    return item.kind === 'submenu' ? flatten((item as { items: readonly MenuItem[] }).items) : []
  })
}

/**
 * One group per top-level menu, in menu order; a nested submenu (Developer)
 * joins its parent. An id appears once, in the first group that has it; a menu
 * left with no action is dropped. `extra`: actions with a shortcut but no menu
 * item on this platform, added to the menu named by `menu` (a new group at the
 * end when the menu is not there).
 */
export function groupMenuActions(
  submenus: readonly { label: string; items: readonly MenuItem[] }[],
  extra: readonly { menu: string; action: MenuAction }[] = [],
): MenuActionGroup[] {
  const groups = submenus.map((s) => ({ menu: s.label, actions: flatten(s.items) }))
  for (const { menu, action } of extra) {
    const group = groups.find((g) => g.menu === menu)
    if (group) group.actions.push(action)
    else groups.push({ menu, actions: [action] })
  }
  const seen = new Set<string>()
  return groups
    .map((g) => ({ menu: g.menu, actions: g.actions.filter((a) => !seen.has(a.id) && seen.add(a.id)) }))
    .filter((g) => g.actions.length > 0)
}
