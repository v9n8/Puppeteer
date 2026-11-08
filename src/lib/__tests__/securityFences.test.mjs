// The security fences of the selection bar and ⌘F (lib/selectionSurface.ts,
// lib/pageFind.ts), pinned so a later change cannot quietly drop them:
// - the Secret vault window gets neither the bar nor ⌘F, under every label the
//   frontend opens it with and the backend lets into the vault;
// - with `selection.translate` off, the bar never reaches the translation model.
// Run: node --test src/lib/__tests__/securityFences.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { canTranslate, isSelectionExcludedWindow, selectionActions } from '../selectionSurface.ts'
import { pickFindArea } from '../pageFind.ts'
import { toRemoteLabel } from '../ipc/windows.ts'

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')

function match(source, re, what) {
  const m = source.match(re)
  assert.ok(m, `${what} not found`)
  return m[1]
}

const prop = (id, effective) => ({
  id,
  stored: null,
  effective,
  locked: false,
  default: effective,
  degraded: [],
  reason_key: null,
})

const SELECTION_PROPS = [
  'text_tools.selection.quote',
  'text_tools.selection.translate',
  'text_tools.selection.copy_md',
  'text_tools.selection.save_note',
  'text_tools.selection.save_todo',
]

/** Every selection prop on, minus `off`; `textToolsOn` is the module switch. */
function tree({ off = [], textToolsOn = true } = {}) {
  return [
    {
      module: 'text_tools',
      enabled: textToolsOn,
      locked: false,
      props: SELECTION_PROPS.map((id) => prop(id, textToolsOn && !off.includes(id))),
    },
    {
      module: 'productivity',
      enabled: true,
      locked: false,
      props: ['productivity.notes', 'productivity.todos'].map((id) => prop(id, true)),
    },
  ]
}

const ALLOW_ALL = { quote: true, translate: true, copyMd: true, saveNote: true, saveTodo: true }

// ── Which windows are the vault ─────────────────────────────────────────────

const LOCAL_LABEL = match(
  read('../secretNotesWindow.ts'),
  /SECRET_NOTES_WINDOW_LABEL = '([^']+)'/,
  'SECRET_NOTES_WINDOW_LABEL',
)
const URL_FLAG = match(
  read('../secretNotesWindow.ts'),
  /new URLSearchParams\(\{ (\w+): '1' \}\)/,
  'the vault window URL flag',
)
const vaultRs = read('../../../src-tauri/src/commands/secret_vault.rs')
const BACKEND_LABELS = [
  match(vaultRs, /const SECRET_NOTES_WINDOW: &str = "([^"]+)"/, 'SECRET_NOTES_WINDOW'),
  match(vaultRs, /const REMOTE_SECRET_NOTES_WINDOW: &str = "([^"]+)"/, 'REMOTE_SECRET_NOTES_WINDOW'),
]

/** `(search, label)` of the vault window, opened locally and from a remote desktop. */
const VAULT_WINDOWS = [
  [`?${URL_FLAG}=1`, LOCAL_LABEL],
  [`?${URL_FLAG}=1&remote=1`, toRemoteLabel(LOCAL_LABEL)],
  // Either half alone is enough: a URL flag lost on reload, a label reused.
  [`?${URL_FLAG}=1&projectId=p&id=n`, null],
  ...BACKEND_LABELS.map((label) => ['', label]),
]

test('the vault window labels agree between frontend and backend', () => {
  assert.equal(URL_FLAG, 'secretNotesWindow')
  assert.deepEqual(BACKEND_LABELS, [LOCAL_LABEL, toRemoteLabel(LOCAL_LABEL)])
})

test('every way the vault window is opened is excluded', () => {
  for (const [search, label] of VAULT_WINDOWS) {
    assert.equal(isSelectionExcludedWindow(search, label), true, `${search} ${label}`)
  }
})

test('the vault window gets no bar, whatever the tree and surface', () => {
  for (const [search, label] of VAULT_WINDOWS) {
    const excludedWindow = isSelectionExcludedWindow(search, label)
    assert.deepEqual(selectionActions(tree(), ALLOW_ALL, { excludedWindow, hasQuoteTarget: true }), [], label)
    assert.equal(canTranslate(tree(), ALLOW_ALL, excludedWindow), false, label)
  }
})

test('the vault window gets no ⌘F, even with an area focused', () => {
  const root = { contains: (n) => n === root || n === text }
  const text = {}
  const area = { root }
  for (const [search, label] of VAULT_WINDOWS) {
    const excludedWindow = isSelectionExcludedWindow(search, label)
    const ctx = { excludedWindow, overlay: null, hints: [text] }
    assert.equal(pickFindArea([area], (a) => a.root, ctx), null, label)
  }
  // The same area in an ordinary window does take it.
  const ctx = { excludedWindow: isSelectionExcludedWindow('', 'main'), overlay: null, hints: [text] }
  assert.equal(pickFindArea([area], (a) => a.root, ctx), area)
})

test('the bar and ⌘F both ask isSelectionExcludedWindow about their own window', () => {
  const call = 'isSelectionExcludedWindow(window.location.search, windowLabel())'
  assert.ok(read('../../components/SelectionToolbar.vue').includes(call), 'SelectionToolbar.vue')
  assert.ok(read('../../composables/usePageFind.ts').includes(call), 'usePageFind.ts')
})

// ── Translate off: the bar never reaches the model ──────────────────────────

test('canTranslate: only while the Translate button would show', () => {
  assert.equal(canTranslate(tree(), ALLOW_ALL, false), true)
  assert.equal(canTranslate(tree({ off: ['text_tools.selection.translate'] }), ALLOW_ALL, false), false)
  assert.equal(canTranslate(tree({ textToolsOn: false }), ALLOW_ALL, false), false)
  assert.equal(canTranslate(tree(), { ...ALLOW_ALL, translate: false }, false), false)
  assert.equal(canTranslate(tree(), { copyMd: true }, false), false)
})

test('canTranslate: other switches and a missing quote target do not matter', () => {
  const others = SELECTION_PROPS.filter((id) => id !== 'text_tools.selection.translate')
  assert.equal(canTranslate(tree({ off: others }), { translate: true }, false), true)
})

test('canTranslate agrees with the bar on every switch combination', () => {
  for (let mask = 0; mask < 1 << SELECTION_PROPS.length; mask++) {
    const off = SELECTION_PROPS.filter((_, i) => mask & (1 << i))
    for (const quote of [true, false]) {
      const shown = selectionActions(tree({ off }), ALLOW_ALL, { excludedWindow: false, hasQuoteTarget: quote })
      assert.equal(canTranslate(tree({ off }), ALLOW_ALL, false), shown.includes('translate'), off.join())
    }
  }
})

test('the bar warms and opens a translation only through canTranslate', () => {
  const bar = read('../../components/SelectionToolbar.vue')
  assert.ok(!bar.includes('translate_text'), 'the bar never sends text itself (TranslatePopover does)')
  const prewarms = bar.split('\n').filter((l) => l.includes('prewarm_translate'))
  assert.ok(prewarms.length > 0, 'prewarm call found')
  for (const line of prewarms) assert.ok(line.includes('canTranslate('), line.trim())
  // TranslatePopover calls translate_text on mount and on every language switch.
  assert.match(bar, /<TranslatePopover\s+v-if="openTranslation"/)
  assert.match(bar, /const openTranslation = computed\([\s\S]*?canTranslate\(/)
})

test('the bar tags its translate calls so the backend can refuse them', () => {
  // The backend gates `source: 'selection'` on text_tools.selection.translate.
  const bar = read('../../components/SelectionToolbar.vue')
  for (const line of bar.split('\n').filter((l) => l.includes('prewarm_translate'))) {
    assert.ok(line.includes("source: 'selection'"), line.trim())
  }
  const popover = read('../../components/TranslatePopover.vue')
  assert.match(popover, /invoke<number>\('translate_text', \{[^}]*source: 'selection'/)
  // Calendar translates through lib/translate.ts, outside the switch.
  assert.ok(!read('../translate.ts').includes("'selection'"))
})
