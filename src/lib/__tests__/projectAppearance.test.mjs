// Unit tests for lib/projectAppearance.ts (project colour / icon / hidden, UX-08).
// Run: node --test src/lib/__tests__/projectAppearance.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { FolderOpen, Rocket } from 'lucide-vue-next'
import {
  PROJECT_COLORS, PROJECT_ICONS, projectColor, projectIcon, projectTextClass,
  projectTileClass, swatchClass, visibleProjects, fullListMove,
} from '../projectAppearance.ts'

/** A `pub(crate) const NAME: &[&str] = &[ ... ];` list from projects.rs. */
function rustList(name) {
  const src = readFileSync(new URL('../../../src-tauri/src/commands/projects.rs', import.meta.url), 'utf8')
  const body = src.match(new RegExp(`const ${name}: &\\[&str\\] = &\\[([^\\]]*)\\]`))
  assert.ok(body, `${name} not found in projects.rs`)
  return [...body[1].matchAll(/"([^"]+)"/g)].map((m) => m[1])
}

test('the allow-lists match the backend, which refuses anything else', () => {
  assert.deepEqual([...PROJECT_COLORS], rustList('PROJECT_COLORS'))
  assert.deepEqual([...PROJECT_ICONS], rustList('PROJECT_ICONS'))
})

test('every colour has its classes and every icon a component', () => {
  for (const c of PROJECT_COLORS) {
    assert.match(swatchClass(c), /^bg-/)
    assert.match(projectTextClass(c), /^text-/)
    assert.ok(projectTileClass(c))
  }
  for (const i of PROJECT_ICONS) assert.notEqual(projectIcon(i), FolderOpen, i)
})

test('unset or unknown values fall back to the default look', () => {
  assert.equal(projectColor('info'), 'info')
  for (const v of [null, undefined, '', '#ff0000', 'red']) {
    assert.equal(projectColor(v), null)
    assert.equal(projectTextClass(v), '')
    assert.equal(projectTileClass(v), projectTileClass('primary'))
  }
  assert.equal(projectIcon('rocket'), Rocket)
  for (const v of [null, undefined, '', 'skull']) assert.equal(projectIcon(v), FolderOpen)
})

const P = (id, hidden = false) => ({ id, hidden })

test('visibleProjects: hidden ones only while the toggle is on', () => {
  const all = [P('a'), P('b', true), P('c')]
  assert.deepEqual(visibleProjects(all, false).map((p) => p.id), ['a', 'c'])
  assert.deepEqual(visibleProjects(all, true).map((p) => p.id), ['a', 'b', 'c'])
})

/** Apply a move like the store's reorder (splice out, splice in). */
function move(list, [from, to]) {
  const out = list.slice()
  const [m] = out.splice(from, 1)
  out.splice(to, 0, m)
  return out.map((p) => p.id)
}

test('fullListMove: a drag among shown projects reorders the full list the same way', () => {
  const all = [P('a'), P('h1', true), P('b'), P('c'), P('h2', true), P('d')]
  const shown = visibleProjects(all, false) // a b c d
  // a → onto c (down): a lands after c.
  assert.deepEqual(move(all, fullListMove(all, shown, 0, 2)), ['h1', 'b', 'c', 'a', 'h2', 'd'])
  // d → onto b (up): d lands before b.
  assert.deepEqual(move(all, fullListMove(all, shown, 3, 1)), ['a', 'h1', 'd', 'b', 'c', 'h2'])
  // With every project shown the indexes are the same.
  assert.deepEqual(fullListMove(all, all, 1, 4), [1, 4])
})
