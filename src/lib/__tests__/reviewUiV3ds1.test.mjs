// UI review v3, design round DS-1 (compact account menu + one usage block,
// option A): source-level checks of template rules that have no pure function
// to test, in the style of reviewUiV3ds3.test.mjs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const json = (p) => JSON.parse(read(p))

test('UR-55: the account menu lists names only — no usage figures, email, "Default" row or Manage accounts', () => {
  const chips = read('../../components/sessions/RunComposerChips.vue')
  assert.match(chips, /w-\[240px\][^"]*p-1/)
  assert.match(chips, /<DropdownLabel class="px-2 pb-1 pt-1">/)
  assert.match(chips, /flex h-8 w-full items-center gap-2 rounded px-2 text-left/)
  assert.match(chips, /<Star class="h-3 w-3 fill-primary text-primary"/)
  assert.match(chips, /t\('run\.accountMenu\.expiredShort'\)/)
  assert.match(chips, /accountMenuRows\(/)
  assert.doesNotMatch(chips, /accountUsage|followsProject|run\.accountMenu\.manage|a\.detail/)
  const composer = read('../../components/sessions/useRunComposer.ts')
  assert.doesNotMatch(composer, /defaultClaudeAccountLabel|claudeAccountDefaultWith/)
  assert.doesNotMatch(read('../../components/sessions/useComposerChips.ts'), /accountUsage/)
  for (const lang of ['en', 'vi']) {
    const run = json(`../../i18n/locales/${lang}/run.json`)
    assert.equal(typeof run.accountMenu.expiredShort, 'string', lang)
    assert.equal(run.accountMenu.followsProject, undefined, lang)
    assert.equal(run.claudeAccountDefaultWith, undefined, lang)
  }
})
