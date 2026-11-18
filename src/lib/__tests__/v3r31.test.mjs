import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const flagItems = read('../../components/sessions/FlagMenuItems.vue')
const runItems = read('../../components/RunActionItems.vue')
const boardActions = read('../../components/board/useBoardRunActions.ts')
const overlay = read('../../components/windows/WindowsOverlay.vue')
const sidebar = read('../../components/listDetail/ListDetailSidebar.vue')
const server = read('../../components/servers/ServerDetail.vue')
const schedules = read('../../views/SchedulesView.vue')

// The menu is unmounted by the time setRunFlag resolves, and Vue drops an emit from an
// unmounted component: the board must be told through a callback prop instead.
test('FlagMenuItems reports a saved flag through a callback prop, not emit', () => {
  assert.doesNotMatch(flagItems, /defineEmits/)
  assert.match(flagItems, /onChanged\?: \(flagId: string \| null\) => void/)
  assert.match(flagItems, /props\.onChanged\?\.\(flagId\)/)
})

test('RunActionItems hands the board callback to FlagMenuItems as a prop', () => {
  assert.doesNotMatch(runItems, /flagChanged: \[\]/)
  assert.match(runItems, /onFlagChanged\?: \(\) => void/)
  assert.match(runItems, /:on-changed="\(\) => props\.onFlagChanged\?\.\(\)"/)
  assert.match(boardActions, /flagChanged: \(\) => onChanged\(\)/)
})

test('Windows overlay: closing / minimizing one card toasts a failure', () => {
  assert.match(overlay, /toast\.error\(t\('windows\.closeFailed'\)\)/)
  assert.match(overlay, /toast\.error\(t\('windows\.minimizeFailed'\)\)/)
})

test('List-detail group title button is 28px high itself', () => {
  assert.match(sidebar, /class="flex min-h-7 min-w-0 flex-1 cursor-pointer items-center gap-1\.5 text-left/)
})

test('Server detail: auth switch, remove-tag button and tag box reach 28px', () => {
  assert.match(server, /<SegmentedControl v-model="authMethod"[^>]*\[&_button\]:min-h-7/)
  assert.match(server, /class="flex h-7 w-7 cursor-pointer items-center justify-center rounded text-muted-foreground/)
  assert.match(server, /class="h-7 min-w-24 flex-1 bg-transparent/)
})

test('Schedules: "Open session" link is 28px high', () => {
  assert.match(schedules, /class="ml-1 inline-flex min-h-7 items-center px-1 underline hover:text-foreground"/)
})
