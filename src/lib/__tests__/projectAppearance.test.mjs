// Unit tests for lib/projectAppearance.ts (project colour / icon / hidden, UX-08).
// Run: node --test src/lib/__tests__/projectAppearance.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { FolderOpen, Rocket } from 'lucide-vue-next'
import {
  PROJECT_COLORS, PROJECT_ICONS, projectColor, projectIcon, projectTextClass,
  projectTileClass, swatchClass, visibleProjects, fullListMove,
} from '../projectAppearance.ts'
import { contrastRatio, mergeThemeBlocks, parseHsl, parseThemeBlocks, resolveTheme } from '../themeContrast.ts'

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

// Tag text (TILE) on its 10% tint over the card; neutral sits on bg-muted instead.
const css = (f) => readFileSync(new URL(`../../assets/${f}`, import.meta.url), 'utf8')
const themeFiles = readdirSync(new URL('../../assets/themes/', import.meta.url)).filter((f) => f.endsWith('.css'))
const themes = mergeThemeBlocks([css('main.css'), ...themeFiles.map((f) => css(`themes/${f}`))].map(parseThemeBlocks))
const STATUS = ['primary', 'info', 'success', 'warning', 'destructive']
function tagContrast(id, mode, color) {
  const t = resolveTheme(themes, id, mode)
  const fg = parseHsl(t[color === 'neutral' ? 'muted-foreground' : STATUS.includes(color) ? color : `tag-${color}`])
  const card = parseHsl(t.card)
  const bg = color === 'neutral' ? parseHsl(t.muted) : fg.map((v, i) => Math.round(v * 0.1 + card[i] * 0.9))
  return contrastRatio(fg, bg)
}

test('the twelve colours run round the colour wheel, the order the picker grid shows', () => {
  assert.deepEqual([...PROJECT_COLORS], [
    'neutral', 'primary', 'info', 'teal', 'success', 'lime',
    'yellow', 'warning', 'orange', 'destructive', 'pink', 'violet',
  ])
  assert.equal(projectTileClass('teal'), 'bg-tag-teal/10 border-tag-teal/20 text-tag-teal')
})

test('tag text reads at 4.5:1 on its tint in light and dark', () => {
  for (const mode of ['light', 'dark']) {
    for (const c of PROJECT_COLORS) {
      const ratio = tagContrast('default', mode, c)
      assert.ok(ratio >= 4.5, `${mode} ${c} ${ratio.toFixed(2)}`)
    }
  }
  // The new tag tokens are not themed, so they must also pass on every light theme's card.
  for (const id of Object.keys(themes.themes)) {
    for (const c of PROJECT_COLORS.filter((x) => x !== 'neutral' && !STATUS.includes(x))) {
      const ratio = tagContrast(id, 'light', c)
      assert.ok(ratio >= 4.5, `${id} light ${c} ${ratio.toFixed(2)}`)
    }
  }
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
