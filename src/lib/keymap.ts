// Keymap (prop `app.command_palette`): the keyboard shortcut of each native
// menu action, by action id (`go./skills`, `view.commandPalette`, the ids the
// command palette uses too). Kept free of Vue / Tauri / `@/` imports so the
// helpers can be unit-tested with `node --test`; lib/appMenu.ts puts the result
// on the menu items as Tauri accelerators, so the OS fires them and the menu
// shows them.
//
// A shortcut is stored in Tauri's accelerator syntax, in one canonical form:
// modifiers `CmdOrCtrl`, `Ctrl`, `Alt`, `Shift` in that order, then the key
// (`A`–`Z`, `0`–`9`, `F1`–`F24` or one of `` ,./;'[]\-=` ``). The key is the
// physical one (`KeyboardEvent.code`), as Tauri matches it.

/**
 * Settings row holding the user's changes as JSON: `{ "<action id>": "<accelerator>" }`,
 * `""` for "no shortcut". Actions left out keep their default. Every `keymap.*`
 * row travels in a settings backup (`PORTABLE_SETTING_PREFIXES`, settings.rs).
 */
export const KEYMAP_SETTING_KEY = 'keymap.config'

/** Menu action opening the command palette. */
export const PALETTE_ACTION = 'view.commandPalette'

/** Shortcuts before any change. Every other action starts with none. */
export const DEFAULT_KEYMAP: Readonly<Record<string, string>> = {
  'go./settings': 'CmdOrCtrl+,',
  [PALETTE_ACTION]: 'CmdOrCtrl+K',
}

/** Action id → accelerator (`""`: no shortcut), only where it differs from the default. */
export type Keymap = Record<string, string>

/** Why a shortcut can't be used. */
export type AcceleratorProblem = 'invalid' | 'needsModifier' | 'reserved'

const MODIFIERS = ['CmdOrCtrl', 'Ctrl', 'Alt', 'Shift'] as const
type Modifier = (typeof MODIFIERS)[number]

const MODIFIER_ALIASES: Readonly<Record<string, Modifier>> = {
  cmdorctrl: 'CmdOrCtrl',
  cmdorcontrol: 'CmdOrCtrl',
  commandorctrl: 'CmdOrCtrl',
  commandorcontrol: 'CmdOrCtrl',
  ctrl: 'Ctrl',
  control: 'Ctrl',
  alt: 'Alt',
  option: 'Alt',
  shift: 'Shift',
}

// `KeyboardEvent.code` of each punctuation key → its accelerator spelling.
const PUNCTUATION_CODES: Readonly<Record<string, string>> = {
  Comma: ',',
  Period: '.',
  Slash: '/',
  Semicolon: ';',
  Quote: "'",
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Minus: '-',
  Equal: '=',
  Backquote: '`',
}
const PUNCTUATION = new Set(Object.values(PUNCTUATION_CODES))

const isFunctionKey = (key: string) => /^F([1-9]|1[0-9]|2[0-4])$/.test(key)

// Enter, Tab, arrows, Space, Backspace… are left out on purpose: with ⌘ / Ctrl
// they already edit text or move the caret, and a menu accelerator would take
// them from every text field.
function canonicalKey(token: string): string | null {
  const upper = token.toUpperCase()
  if (/^[A-Z0-9]$/.test(upper) || isFunctionKey(upper)) return upper
  return PUNCTUATION.has(token) ? token : null
}

/**
 * `raw` in the canonical form, or null when Tauri's syntax or this keymap does
 * not accept it. Modifier aliases (`CommandOrControl`, `Option`, …) and any
 * case are accepted; `Cmd` / `Super` alone are not (the Windows key elsewhere).
 */
export function normalizeAccelerator(raw: string): string | null {
  const tokens = raw.split('+').map((t) => t.trim())
  if (tokens.length === 0 || tokens.some((t) => !t)) return null
  const key = canonicalKey(tokens[tokens.length - 1])
  if (!key) return null
  const mods = new Set<Modifier>()
  for (const token of tokens.slice(0, -1)) {
    const mod = MODIFIER_ALIASES[token.toLowerCase()]
    if (!mod) return null
    mods.add(mod)
  }
  return [...MODIFIERS.filter((m) => mods.has(m)), key].join('+')
}

/**
 * The accelerator a key press spells, or null when its key can't be bound (a
 * lone modifier, an arrow, Enter…). ⌘ is `CmdOrCtrl` on macOS, Ctrl elsewhere;
 * the Windows / Super key is never part of one.
 */
export function acceleratorFromEvent(
  e: { code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean },
  isMac: boolean,
): string | null {
  const key = e.code.startsWith('Key') || e.code.startsWith('Digit')
    ? e.code.replace(/^(Key|Digit)/, '')
    : isFunctionKey(e.code)
      ? e.code
      : PUNCTUATION_CODES[e.code]
  if (!key || !canonicalKey(key)) return null
  if (!isMac && e.metaKey) return null
  const mods: Modifier[] = []
  if (isMac ? e.metaKey : e.ctrlKey) mods.push('CmdOrCtrl')
  if (isMac && e.ctrlKey) mods.push('Ctrl')
  if (e.altKey) mods.push('Alt')
  if (e.shiftKey) mods.push('Shift')
  return [...mods, key].join('+')
}

/**
 * What the key combination is on this platform, for comparing two shortcuts:
 * `CmdOrCtrl+K` and `Ctrl+K` are the same keys off macOS, not on it.
 */
function chord(accel: string, isMac: boolean): string {
  const tokens = accel.split('+')
  const key = tokens[tokens.length - 1]
  const mods = new Set(tokens.slice(0, -1))
  const parts = [
    isMac && mods.has('CmdOrCtrl') ? 'Cmd' : '',
    mods.has('Ctrl') || (!isMac && mods.has('CmdOrCtrl')) ? 'Ctrl' : '',
    mods.has('Alt') ? 'Alt' : '',
    mods.has('Shift') ? 'Shift' : '',
  ]
  return [...parts.filter(Boolean), key].join('+')
}

// Keys the system or the app already gives a meaning to, which a menu
// accelerator would take over: quit / close / minimize, the Edit menu, ⌘F
// (find in page, routed per window — usePageFind) and ⌘S (save in the editors).
const RESERVED_EVERYWHERE = [
  'CmdOrCtrl+Q', 'CmdOrCtrl+W', 'CmdOrCtrl+M',
  'CmdOrCtrl+Z', 'CmdOrCtrl+Shift+Z', 'CmdOrCtrl+X', 'CmdOrCtrl+C', 'CmdOrCtrl+V', 'CmdOrCtrl+A',
  'CmdOrCtrl+F', 'CmdOrCtrl+S',
]
const RESERVED_MAC = [
  'CmdOrCtrl+H', 'CmdOrCtrl+Alt+H', 'CmdOrCtrl+Alt+M', 'CmdOrCtrl+Alt+W', 'CmdOrCtrl+Shift+Q',
  'CmdOrCtrl+Ctrl+Q', 'CmdOrCtrl+Ctrl+F', 'CmdOrCtrl+Alt+D', 'CmdOrCtrl+`', 'CmdOrCtrl+Shift+`',
  'CmdOrCtrl+Shift+3', 'CmdOrCtrl+Shift+4', 'CmdOrCtrl+Shift+5', 'CmdOrCtrl+Shift+/',
]
const RESERVED_OTHER = ['CmdOrCtrl+Y', 'Alt+F4']

function reservedChords(isMac: boolean): Set<string> {
  return new Set([...RESERVED_EVERYWHERE, ...(isMac ? RESERVED_MAC : RESERVED_OTHER)].map((a) => chord(a, isMac)))
}

/** Why `accel` can't be a shortcut on this platform, or null when it can. */
export function acceleratorProblem(accel: string, isMac: boolean): AcceleratorProblem | null {
  const canonical = normalizeAccelerator(accel)
  if (!canonical) return 'invalid'
  const tokens = canonical.split('+')
  const key = tokens[tokens.length - 1]
  // A bare letter would fire while typing; ⌥ / ⇧ + a letter types a character.
  // On macOS ⌃ + a letter edits text, so ⌘ is needed there.
  const hasCommand = tokens.includes('CmdOrCtrl') || (!isMac && tokens.includes('Ctrl'))
  if (!hasCommand && !isFunctionKey(key)) return 'needsModifier'
  return reservedChords(isMac).has(chord(canonical, isMac)) ? 'reserved' : null
}

/** How the menu shows it: `⌥⇧⌘K` on macOS, `Ctrl+Alt+Shift+K` elsewhere. */
export function formatAccelerator(accel: string, isMac: boolean): string {
  const tokens = accel.split('+')
  const key = tokens[tokens.length - 1]
  const mods = new Set(tokens.slice(0, -1))
  if (isMac) {
    // Apple's order: ⌃ ⌥ ⇧ ⌘.
    const symbols = [mods.has('Ctrl') ? '⌃' : '', mods.has('Alt') ? '⌥' : '', mods.has('Shift') ? '⇧' : '', mods.has('CmdOrCtrl') ? '⌘' : '']
    return symbols.join('') + key
  }
  return chord(accel, false)
}

/**
 * Parse the `keymap.config` setting. Never throws: a missing or malformed value
 * is no change at all, and an entry that isn't a valid accelerator is dropped.
 */
export function parseKeymap(raw: string | null | undefined): Keymap {
  if (!raw) return {}
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    return {}
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {}
  const keymap: Keymap = {}
  for (const [id, value] of Object.entries(parsed)) {
    if (!id || typeof value !== 'string') continue
    if (value === '') keymap[id] = ''
    else {
      const accel = normalizeAccelerator(value)
      if (accel) keymap[id] = accel
    }
  }
  return keymap
}

export function serializeKeymap(keymap: Keymap): string {
  return JSON.stringify(keymap)
}

/**
 * The shortcut of each action in `actionIds` (the menu's items, in menu order):
 * the user's own first, then the defaults of the actions they left alone. One
 * that can't be used here (reserved, invalid) or whose keys an earlier one
 * already holds is dropped — a stored value may come from a backup made on
 * another OS or from a hand edit, which the Settings screen never checked.
 */
export function resolveKeymap(actionIds: readonly string[], keymap: Keymap, isMac: boolean): Map<string, string> {
  const taken = new Set<string>()
  const resolved = new Map<string, string>()
  const assign = (id: string, accel: string) => {
    if (!accel || resolved.has(id) || acceleratorProblem(accel, isMac)) return
    const canonical = normalizeAccelerator(accel) as string
    const keys = chord(canonical, isMac)
    if (taken.has(keys)) return
    taken.add(keys)
    resolved.set(id, canonical)
  }
  const changed = (id: string) => Object.prototype.hasOwnProperty.call(keymap, id)
  for (const id of actionIds) if (changed(id)) assign(id, keymap[id])
  for (const id of actionIds) if (!changed(id) && DEFAULT_KEYMAP[id]) assign(id, DEFAULT_KEYMAP[id])
  return resolved
}

/** The other action already on `accel`'s keys, if any. */
export function keymapConflict(resolved: ReadonlyMap<string, string>, actionId: string, accel: string, isMac: boolean): string | null {
  const keys = chord(accel, isMac)
  for (const [id, other] of resolved) {
    if (id !== actionId && chord(other, isMac) === keys) return id
  }
  return null
}

/** `keymap` with `actionId` on `accel` (`""`: no shortcut). Back to the default, the entry goes. */
export function setBinding(keymap: Keymap, actionId: string, accel: string): Keymap {
  const next = { ...keymap }
  if (accel === (DEFAULT_KEYMAP[actionId] ?? '')) delete next[actionId]
  else next[actionId] = accel
  return next
}

/** `keymap` with `actionId` back to its default shortcut. */
export function resetBinding(keymap: Keymap, actionId: string): Keymap {
  const next = { ...keymap }
  delete next[actionId]
  return next
}

/** Whether a key press is `accel`, for a window without the native menu. */
export function matchesAccelerator(
  e: { code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean },
  accel: string,
  isMac: boolean,
): boolean {
  const pressed = acceleratorFromEvent(e, isMac)
  return pressed !== null && chord(pressed, isMac) === chord(accel, isMac)
}
