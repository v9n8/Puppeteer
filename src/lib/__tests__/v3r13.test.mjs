// Regression checks for V3-R13 (findings F-U1-R4-3,4,7,8,9,10). The components
// are Vue SFCs, so the markup rules are checked on the source.
// Run: node --test src/lib/__tests__/v3r13.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { navActive } from '../sidebarNav.ts'

const src = (p) => readFileSync(new URL(`../../${p}`, import.meta.url), 'utf8')

test('F-3: no class passed to AddTodoButton (its root is a fragment)', () => {
  assert.doesNotMatch(src('views/FocusView.vue'), /<AddTodoButton[^>]*\sclass=/)
})

test('F-4: a filter that hides the selected row clears the selection', () => {
  assert.match(src('components/items/ItemListPane.vue'), /watch\(visibleRows,/)
})

test('F-7: the snooze menu treats focus inside the fading menu as lost', () => {
  assert.match(src('components/focus/FocusSnoozeMenu.vue'), /closest\('\.ui-popover'\)/)
})

test('F-8: the window button shows the "opens in window" text only when it does', () => {
  const s = src('components/app/SidebarNavItem.vue')
  assert.match(s, /<span v-if="inOwnWindow" class="sr-only"/)
  assert.doesNotMatch(s, /:title="`\$\{tip\} — \$\{t\('nav\.opensInWindow'\)\}`"/)
  assert.equal(navActive('secret-notes', '/secret-notes'), true)
  assert.equal(navActive('secret-notes', '/projects'), false)
})

test('F-9: Calendar prev/next buttons are the 28px icon-sm size', () => {
  const s = src('components/calendar/CalendarHeaderControls.vue')
  assert.doesNotMatch(s, /size="icon"/)
  assert.equal(s.match(/size="icon-sm"/g).length, 2)
})

test('F-10: the ongoing label shares the time line and the now time sits in the gutter', () => {
  const s = src('components/calendar/CalendarWeekGrid.vue')
  assert.doesNotMatch(s, /<div v-if="isOngoing\(b\.ev, today\)"/)
  assert.doesNotMatch(s, /absolute left-2 -top-2\.5/)
})
