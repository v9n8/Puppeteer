import { shallowReactive } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import { i18n } from '@/i18n'
import { NAV_ROUTES } from '@/lib/navigation'
import { allowsFeature, filterByFeature, menuActionFeature, startsGroup } from '@/lib/featureGate'
import { useFeaturesStore } from '@/stores/features'
import { useAppSettingsStore } from '@/stores/appSettings'
import { PALETTE_FEATURE } from '@/lib/commandPalette'
import {
  NEW_SESSION_ACTION, PALETTE_ACTION, PREVIOUS_PROJECT_ACTION, parseKeymap, resolveKeymap, type Keymap,
} from '@/lib/keymap'

/**
 * Native menu bar (File / Edit / View / Go / Window / Help).
 *
 * Items whose feature switch is off are left out (lib/featureGate), so the menu
 * is also rebuilt on `features:changed`.
 *
 * Tauri ships a default menu with nothing app-specific in it. This module
 * replaces it: the spec below is sent to the `set_app_menu` command, and Rust
 * reports clicks back on `menu://action` carrying the item id. Actions are
 * resolved HERE, in the frontend, because that's where the router, the layout
 * store and the quick-capture overlay live.
 *
 * Labels are built from vue-i18n, so `applyAppMenu()` must be re-run whenever
 * the language — or any state baked into the menu, like "Hide" vs "Show
 * Sidebar" or whether New Session is available — changes.
 *
 * Shortcuts come from the keymap (lib/keymap, Settings → Keyboard shortcuts) as
 * accelerators on the items, so the OS fires them and the menu shows them; a
 * changed shortcut needs `applyAppMenu()` too.
 */

/** Event name emitted by the Rust side; must match `MENU_ACTION_EVENT`. */
const MENU_ACTION_EVENT = 'menu://action'

export const IS_MAC = /mac/i.test(navigator.userAgent)

interface ActionItem {
  kind: 'item'
  id: string
  label: string
  accelerator?: string
  enabled?: boolean
}

type ItemSpec =
  | ActionItem
  | { kind: 'predefined'; role: string; label?: string }
  | { kind: 'submenu'; label: string; items: ItemSpec[] }
  // Replaced in Rust by the open-window list the tray keeps (see tray.rs).
  | { kind: 'windowList' }
  | { kind: 'separator' }

interface MenuSpec {
  submenus: { label: string; items: ItemSpec[] }[]
}

export interface MenuState {
  sidebarHidden: boolean
  focusMode: boolean
}

function t(key: string): string {
  return i18n.global.t(key)
}

/**
 * An OS-provided item, labelled in the app language. Without an explicit label
 * macOS titles these in the *system* language, so an app set to Vietnamese on an
 * English Mac would show "Copy" next to "Ghi chú mới…".
 */
function os(role: string, labelKey: string): ItemSpec {
  return { kind: 'predefined', role, label: t(labelKey) }
}

const SEPARATOR: ItemSpec = { kind: 'separator' }

// Both live in the macOS app menu (and Help / File elsewhere), so the Go menu
// leaves them out rather than listing them twice.
const GO_MENU_EXCLUDED = new Set(['/settings', '/about'])

// False while the feature the action opens is switched off.
function actionAllowed(id: string): boolean {
  return allowsFeature(useFeaturesStore().tree, menuActionFeature(id, NAV_ROUTES))
}

/** Drop the items whose feature is off. */
function gated(items: ItemSpec[]): ItemSpec[] {
  return items.filter((item) => item.kind !== 'item' || actionAllowed(item.id))
}

function goMenuItems(): ItemSpec[] {
  const routes = filterByFeature(
    NAV_ROUTES.filter((r) => !GO_MENU_EXCLUDED.has(r.path)),
    useFeaturesStore().tree,
  )
  return routes.flatMap((route, index): ItemSpec[] => [
    // Same grouping as the sidebar dividers.
    ...(startsGroup(routes, index) ? [SEPARATOR] : []),
    { kind: 'item', id: `go.${route.path}`, label: t(route.labelKey) },
  ])
}

function buildSpec(state: MenuState): MenuSpec {
  const submenus: MenuSpec['submenus'] = []
  // ⌘, by default (lib/keymap): a platform convention for Settings.
  const settings: ItemSpec = { kind: 'item', id: 'go./settings', label: t('menu.settings') }
  const about: ItemSpec = { kind: 'item', id: 'go./about', label: t('menu.about') }

  // macOS puts app-level items (about / settings / services / hide / quit) in a
  // first submenu named after the app. On Windows/Linux that submenu doesn't
  // exist: Settings and Quit ride along in File, About in Help.
  if (IS_MAC) {
    submenus.push({
      label: 'Puppeteer',
      items: [
        // The app's own About page rather than the bare OS panel, so there's
        // one About, not two.
        about,
        SEPARATOR,
        settings,
        SEPARATOR,
        os('services', 'menu.services'),
        SEPARATOR,
        os('hide', 'menu.hide'),
        os('hideOthers', 'menu.hideOthers'),
        os('showAll', 'menu.showAll'),
        SEPARATOR,
        os('quit', 'menu.quit'),
      ],
    })
  }

  submenus.push({
    label: t('menu.file'),
    items: [
      {
        kind: 'item',
        id: NEW_SESSION_ACTION,
        label: t('menu.newSession'),
        // Bound app-wide (App.vue, the current or last-used project) and by the
        // project screen on top; greyed out only before that, never a dead item.
        enabled: isMenuActionBound(NEW_SESSION_ACTION),
      },
      { kind: 'item', id: 'file.openProject', label: t('menu.openProject') },
      { kind: 'item', id: PREVIOUS_PROJECT_ACTION, label: t('menu.previousProject') },
      SEPARATOR,
      ...gated([
        { kind: 'item', id: 'file.newTodo', label: t('menu.newTodo') },
        { kind: 'item', id: 'file.newNote', label: t('menu.newNote') },
      ]),
      SEPARATOR,
      ...(IS_MAC
        ? [os('closeWindow', 'menu.closeWindow')]
        : [settings, SEPARATOR, os('closeWindow', 'menu.closeWindow'), os('quit', 'menu.quit')]),
    ],
  })

  // Not optional: on macOS the system only delivers ⌘C/⌘V/⌘Z to the webview
  // when the menu bar actually contains those items. Dropping this submenu
  // silently kills clipboard shortcuts across the whole app.
  submenus.push({
    label: t('menu.edit'),
    items: [
      os('undo', 'menu.undo'),
      os('redo', 'menu.redo'),
      SEPARATOR,
      os('cut', 'menu.cut'),
      os('copy', 'menu.copy'),
      os('paste', 'menu.paste'),
      os('selectAll', 'menu.selectAll'),
    ],
  })

  // Layout toggles only; things that open a window live under Window.
  const palette = gated([{ kind: 'item', id: PALETTE_ACTION, label: t('menu.commandPalette') }])
  submenus.push({
    label: t('menu.view'),
    items: [
      ...(palette.length ? [...palette, SEPARATOR] : []),
      {
        kind: 'item',
        id: 'view.toggleSidebar',
        label: state.sidebarHidden ? t('menu.showSidebar') : t('menu.hideSidebar'),
      },
      {
        kind: 'item',
        id: 'view.toggleFocus',
        label: state.focusMode ? t('menu.exitFocusMode') : t('menu.enterFocusMode'),
      },
      SEPARATOR,
      // The OS full-screen item only exists on macOS; elsewhere it's a no-op,
      // so the window is toggled from the frontend instead.
      IS_MAC
        ? os('fullscreen', 'menu.fullscreen')
        : { kind: 'item', id: 'view.toggleFullscreen', label: t('menu.fullscreen') },
    ],
  })

  submenus.push({ label: t('menu.go'), items: goMenuItems() })

  const featureWindows = gated([
    { kind: 'item', id: 'window.todoList', label: t('menu.todoList') },
    { kind: 'item', id: 'window.noteList', label: t('menu.noteList') },
    { kind: 'item', id: 'window.secretNotes', label: t('menu.secretNotes') },
    { kind: 'item', id: 'window.slackThreads', label: t('menu.slackThreads') },
  ])
  submenus.push({
    label: t('menu.window'),
    items: [
      os('minimize', 'menu.minimize'),
      os('maximize', IS_MAC ? 'menu.zoom' : 'menu.maximize'),
      SEPARATOR,
      // No doubled separator when every one of them is off.
      ...(featureWindows.length ? [...featureWindows, SEPARATOR] : []),
      { kind: 'windowList' },
      ...(IS_MAC ? [SEPARATOR, os('bringAllToFront', 'menu.bringAllToFront')] : []),
    ],
  })

  submenus.push({
    label: t('menu.help'),
    items: [
      { kind: 'item', id: 'help.docs', label: t('menu.docs') },
      { kind: 'item', id: 'help.reportIssue', label: t('menu.reportIssue') },
      { kind: 'item', id: 'help.openLogs', label: t('menu.openLogs') },
      SEPARATOR,
      // Reload drops all in-memory UI state, so it's tucked away here rather
      // than sitting in View where it's easy to hit by mistake.
      {
        kind: 'submenu',
        label: t('menu.developer'),
        items: [{ kind: 'item', id: 'help.reload', label: t('menu.reload') }],
      },
      ...(IS_MAC ? [] : [SEPARATOR, about]),
    ],
  })

  return { submenus }
}

function actionItems(items: readonly ItemSpec[]): ActionItem[] {
  return items.flatMap((item) =>
    item.kind === 'item' ? [item] : item.kind === 'submenu' ? actionItems(item.items) : [],
  )
}

/** The user's shortcuts; with the keymap's prop off, every action keeps its default. */
function keymapChanges(): Keymap {
  if (!allowsFeature(useFeaturesStore().tree, PALETTE_FEATURE)) return {}
  return parseKeymap(useAppSettingsStore().settings?.keymap_config)
}

let lastState: MenuState = { sidebarHidden: false, focusMode: false }
let lastKeymap: ReadonlyMap<string, string> = new Map()
let nativeMenu = false
let acceleratorsSuspended = false

/**
 * The menu's actions, each once, in menu order (those whose feature is off are
 * not in it). Reactive: follows the language, the feature switches and the
 * screens that bind an action.
 */
export function menuActions(): { id: string; label: string }[] {
  const seen = new Set<string>()
  return actionItems(buildSpec(lastState).submenus.flatMap((s) => s.items))
    .filter((item) => !seen.has(item.id) && seen.add(item.id))
    .map(({ id, label }) => ({ id, label }))
}

/** The shortcut of each menu action, as the menu has them (lib/keymap `resolveKeymap`). Reactive. */
export function effectiveKeymap(): Map<string, string> {
  return resolveKeymap(menuActions().map((a) => a.id), keymapChanges(), IS_MAC)
}

/** The shortcut `id` had when the menu was last installed. */
export function menuAccelerator(id: string): string | undefined {
  return lastKeymap.get(id)
}

/** Whether the OS menu bar is installed, so its accelerators fire (false in a browser). */
export function hasNativeMenu(): boolean {
  return nativeMenu
}

/**
 * Take every shortcut off the menu, or put them back. While Settings records a
 * new shortcut, the keys must reach the page instead of firing an action.
 */
export function suspendAccelerators(on: boolean): void {
  if (acceleratorsSuspended === on) return
  acceleratorsSuspended = on
  void applyAppMenu(lastState)
}

/** Install (or re-install) the menu for the given layout state. */
export async function applyAppMenu(state: MenuState): Promise<void> {
  lastState = state
  const spec = buildSpec(state)
  const items = actionItems(spec.submenus.flatMap((s) => s.items))
  lastKeymap = resolveKeymap([...new Set(items.map((item) => item.id))], keymapChanges(), IS_MAC)
  if (!acceleratorsSuspended) {
    for (const item of items) item.accelerator = lastKeymap.get(item.id)
  }
  try {
    await invoke('set_app_menu', { spec })
    nativeMenu = true
  } catch {
    // Outside the Tauri shell (browser dev) there is no menu bar to set.
  }
}

// ── Action dispatch ─────────────────────────────────────────────────────────

type MenuHandler = () => void

// Reactive so the menu can grey out an item whose screen isn't mounted (see
// `isMenuActionBound`).
const handlers = shallowReactive(new Map<string, MenuHandler>())
// Handlers a later registration of the same id sits on top of, oldest first.
const shadowed = new Map<string, MenuHandler[]>()

/**
 * Bind an action id to a handler for as long as a component is alive. Returns
 * the unbind function. The latest binding wins and unbinding it brings back the
 * one beneath; a `fallback` goes beneath every other binding, whenever it is
 * made. RunView's New Session (which keeps the composer's engine choice) covers
 * the app-wide fallback while that screen is open — even though the screen
 * mounts before App.vue binds its actions.
 */
export function registerMenuAction(id: string, handler: MenuHandler, opts: { fallback?: boolean } = {}): () => void {
  const current = handlers.get(id)
  if (current && opts.fallback) shadowed.set(id, [handler, ...(shadowed.get(id) ?? [])])
  else {
    if (current) shadowed.set(id, [...(shadowed.get(id) ?? []), current])
    handlers.set(id, handler)
  }
  return () => {
    const stack = shadowed.get(id) ?? []
    if (handlers.get(id) === handler) {
      const restored = stack.pop()
      if (restored) handlers.set(id, restored)
      else handlers.delete(id)
    } else {
      // A later binding is still on top (a screen remounting): drop this one from beneath it.
      const i = stack.lastIndexOf(handler)
      if (i >= 0) stack.splice(i, 1)
    }
    if (stack.length === 0) shadowed.delete(id)
  }
}

/** Whether some live component currently handles `id`. Reactive. */
export function isMenuActionBound(id: string): boolean {
  return handlers.has(id)
}

/** Run a menu action as if its item was clicked (the command palette runs them too). */
export function runMenuAction(id: string): void {
  // The menu (or tray) may still show an item whose feature was just turned off.
  if (!actionAllowed(id)) return
  handlers.get(id)?.()
}

/** Subscribe to menu clicks coming from Rust. Returns an unlisten function. */
export async function listenMenuActions(): Promise<UnlistenFn> {
  return listen<string>(MENU_ACTION_EVENT, (event) => {
    if (typeof event.payload === 'string') runMenuAction(event.payload)
  })
}
