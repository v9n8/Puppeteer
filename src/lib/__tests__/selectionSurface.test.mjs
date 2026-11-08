// Unit tests for lib/selectionSurface.ts (the shared selection toolbar's rules).
// Run: node --test src/lib/__tests__/selectionSurface.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  ACTION_FEATURES,
  SELECTION_ACTIONS,
  isSelectionExcludedWindow,
  pickSurface,
  scrollMovesSurface,
  selectionActions,
  toolbarPosition,
} from '../selectionSurface.ts'

const prop = (id, effective) => ({
  id,
  stored: null,
  effective,
  locked: false,
  default: effective,
  degraded: [],
  reason_key: null,
})

/** A tree with the given props on (every other selection prop off). */
function treeWith(on, { textToolsOn = true } = {}) {
  const p = (id) => prop(id, textToolsOn || !id.startsWith('text_tools.') ? on.includes(id) : false)
  return [
    {
      module: 'text_tools',
      enabled: textToolsOn,
      locked: false,
      props: [
        'text_tools.selection.quote',
        'text_tools.selection.translate',
        'text_tools.selection.copy_md',
        'text_tools.selection.save_note',
        'text_tools.selection.save_todo',
      ].map(p),
    },
    {
      module: 'productivity',
      enabled: true,
      locked: false,
      props: ['productivity.notes', 'productivity.todos'].map(p),
    },
  ]
}

const ALL_ON = treeWith([
  'text_tools.selection.quote',
  'text_tools.selection.translate',
  'text_tools.selection.copy_md',
  'text_tools.selection.save_note',
  'text_tools.selection.save_todo',
  'productivity.notes',
  'productivity.todos',
])
const ALLOW_ALL = { quote: true, translate: true, copyMd: true, saveNote: true, saveTodo: true }
const CTX = { excludedWindow: false, hasQuoteTarget: true }

// ── Buttons ─────────────────────────────────────────────────────────────────

test('every action maps to its registry prop', () => {
  assert.deepEqual(Object.keys(ACTION_FEATURES).sort(), [...SELECTION_ACTIONS].sort())
  for (const ids of Object.values(ACTION_FEATURES)) {
    assert.ok(ids[0].startsWith('text_tools.selection.'))
  }
})

test('all on and allowed: every button, in bar order', () => {
  assert.deepEqual(selectionActions(ALL_ON, ALLOW_ALL, CTX), [
    'quote',
    'translate',
    'copyMd',
    'saveNote',
    'saveTodo',
  ])
})

test('only what the surface allows', () => {
  // RunView today: translate + note + todo; FileViewer: translate only.
  assert.deepEqual(
    selectionActions(ALL_ON, { translate: true, saveNote: true, saveTodo: true }, CTX),
    ['translate', 'saveNote', 'saveTodo'],
  )
  assert.deepEqual(selectionActions(ALL_ON, { translate: true }, CTX), ['translate'])
  assert.deepEqual(selectionActions(ALL_ON, { translate: false }, CTX), [])
  assert.deepEqual(selectionActions(ALL_ON, {}, CTX), [])
})

test('selection.translate off: no Translate button', () => {
  const tree = treeWith([
    'text_tools.selection.save_note',
    'productivity.notes',
  ])
  assert.deepEqual(selectionActions(tree, ALLOW_ALL, CTX), ['saveNote'])
})

test('save_note on but productivity.notes off: no Note button', () => {
  const tree = treeWith([
    'text_tools.selection.translate',
    'text_tools.selection.save_note',
    'text_tools.selection.save_todo',
    'productivity.todos',
  ])
  assert.deepEqual(selectionActions(tree, ALLOW_ALL, CTX), ['translate', 'saveTodo'])
})

test('save_todo on but productivity.todos off: no Todo button', () => {
  const tree = treeWith([
    'text_tools.selection.save_note',
    'text_tools.selection.save_todo',
    'productivity.notes',
  ])
  assert.deepEqual(selectionActions(tree, ALLOW_ALL, CTX), ['saveNote'])
})

test('text_tools module off: no button at all', () => {
  const tree = treeWith(['productivity.notes', 'productivity.todos'], { textToolsOn: false })
  assert.deepEqual(selectionActions(tree, ALLOW_ALL, CTX), [])
})

test('a tree not loaded yet (empty) shows nothing', () => {
  assert.deepEqual(selectionActions([], ALLOW_ALL, CTX), [])
})

test('Quote needs a target', () => {
  assert.deepEqual(
    selectionActions(ALL_ON, { quote: true, translate: true }, { ...CTX, hasQuoteTarget: false }),
    ['translate'],
  )
})

test('an excluded window is always empty, whatever the tree and surface', () => {
  assert.deepEqual(selectionActions(ALL_ON, ALLOW_ALL, { ...CTX, excludedWindow: true }), [])
})

// ── Window kind ─────────────────────────────────────────────────────────────

test('the Secret vault window is excluded, by query flag or label', () => {
  assert.equal(isSelectionExcludedWindow('?secretNotesWindow=1', null), true)
  assert.equal(isSelectionExcludedWindow('?secretNotesWindow=1&projectId=p', 'whatever'), true)
  assert.equal(isSelectionExcludedWindow('', 'secret-notes'), true)
  assert.equal(isSelectionExcludedWindow('?remote=1', 'remote-secret-notes'), true)
})

test('other windows are not excluded', () => {
  assert.equal(isSelectionExcludedWindow('', 'main'), false)
  assert.equal(isSelectionExcludedWindow('?fileWindow=1', 'file-viewer'), false)
  assert.equal(isSelectionExcludedWindow('?sessionWindow=1&runId=r', null), false)
  assert.equal(isSelectionExcludedWindow('?secretNotesWindow=0', 'secret-notes-x'), false)
})

// ── Owning surface ──────────────────────────────────────────────────────────

/** A fake DOM node: `contains` walks up the parent chain (self included). */
function node(name, parent = null) {
  const n = {
    name,
    parent,
    contains(other) {
      for (let x = other; x; x = x.parent) if (x === n) return true
      return false
    },
  }
  return n
}

const body = node('body')
const output = node('output', body)
const outputText = node('outputText', output)
const history = node('history', body)
const historyText = node('historyText', history)
const sidebarText = node('sidebarText', body)
const modal = node('modal', output)
const modalText = node('modalText', modal)

const runView = { name: 'run', els: [output, history] }
const nested = { name: 'modal', els: [modal] }
const els = (s) => s.els

test('the surface holding the anchor or the focus owns the selection', () => {
  assert.equal(pickSurface([runView], els, outputText, outputText), runView)
  assert.equal(pickSurface([runView], els, historyText, null), runView)
  // Dragged in from outside: the focus end still lands in the surface.
  assert.equal(pickSurface([runView], els, sidebarText, outputText), runView)
})

test('a selection outside every surface has no owner', () => {
  assert.equal(pickSurface([runView], els, sidebarText, sidebarText), null)
  assert.equal(pickSurface([runView], els, null, null), null)
  assert.equal(pickSurface([], els, outputText, outputText), null)
})

test('an unmounted element (null) never matches', () => {
  const half = { name: 'half', els: [null, history] }
  assert.equal(pickSurface([half], els, outputText, outputText), null)
  assert.equal(pickSurface([half], els, historyText, historyText), half)
})

test('nested surfaces: the innermost wins, whatever the order', () => {
  assert.equal(pickSurface([runView, nested], els, modalText, modalText), nested)
  assert.equal(pickSurface([nested, runView], els, modalText, modalText), nested)
  assert.equal(pickSurface([runView, nested], els, outputText, outputText), runView)
})

// ── Scroll ──────────────────────────────────────────────────────────────────

test('scrolling the document, an ancestor or a scroller inside moves the surface', () => {
  const doc = node('document')
  assert.equal(scrollMovesSurface(doc, [output], doc), true)
  assert.equal(scrollMovesSurface(null, [output], doc), true)
  assert.equal(scrollMovesSurface(output, [output], doc), true)
  assert.equal(scrollMovesSurface(body, [output], doc), true)
  assert.equal(scrollMovesSurface(outputText, [output], doc), true)
})

test('scrolling an unrelated element leaves the bar', () => {
  const doc = node('document')
  assert.equal(scrollMovesSurface(sidebarText, [output], doc), false)
  assert.equal(scrollMovesSurface(history, [output, null], doc), false)
})

// ── Placement ───────────────────────────────────────────────────────────────

const VIEW = { width: 1000, height: 800 }
const BAR = { width: 200, height: 24 }

test('under the selection, left-aligned (6px gap)', () => {
  assert.deepEqual(toolbarPosition({ left: 100, top: 300, bottom: 320 }, BAR, VIEW), { x: 100, y: 326 })
})

test('kept inside the right and left edges', () => {
  assert.deepEqual(toolbarPosition({ left: 950, top: 300, bottom: 320 }, BAR, VIEW), { x: 792, y: 326 })
  assert.deepEqual(toolbarPosition({ left: -40, top: 300, bottom: 320 }, BAR, VIEW), { x: 8, y: 326 })
})

test('flipped above the selection when there is no room below', () => {
  assert.deepEqual(toolbarPosition({ left: 100, top: 700, bottom: 780 }, BAR, VIEW), { x: 100, y: 670 })
})

test('a selection taller than the viewport keeps the bar on screen', () => {
  assert.deepEqual(toolbarPosition({ left: 100, top: -500, bottom: 2000 }, BAR, VIEW), { x: 100, y: 8 })
})

test('an unmeasured bar (size 0) still lands under the selection', () => {
  assert.deepEqual(
    toolbarPosition({ left: 100, top: 300, bottom: 320 }, { width: 0, height: 0 }, VIEW),
    { x: 100, y: 326 },
  )
})
