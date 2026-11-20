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

test('UR-65: the account menu puts ★ before the account name, as AppSelect does', () => {
  const chips = read('../../components/sessions/RunComposerChips.vue')
  const star = chips.indexOf('<Star class="h-3 w-3 fill-primary text-primary"')
  const label = chips.indexOf('{{ a.label }}')
  assert.ok(star > 0 && label > 0 && star < label, 'Star should come before the label')
})

test('UR-57: one usage block — a summary button and a panel with a bordered block per engine', () => {
  const rings = read('../../components/UsageRings.vue')
  assert.match(rings, /<UsageSummary/)
  assert.doesNotMatch(rings, /v-for="r in visibleRows"/)
  // the data, timers and quota toast stay
  assert.match(rings, /budget\.takeQuotaWarnings/)
  assert.match(rings, /setInterval\(reloadAll, 60_000\)/)
  const summary = read('../../components/UsageSummary.vue')
  assert.match(summary, /inline-flex h-7 min-w-0 shrink-0 items-center gap-1\.5 rounded px-1\.5 text-2xs text-muted-foreground/)
  assert.match(summary, /rounded-full border border-border px-1\.5/)
  assert.match(summary, /max-\[901px\]:hidden/)
  assert.match(summary, /summaryRows\(/)
  assert.match(summary, /<Teleport to="body">/)
  const panel = read('../../components/UsagePanel.vue')
  assert.match(panel, /w-\[460px\][^"]*bg-popover p-3/)
  assert.match(panel, /overflow-hidden rounded-md border border-border bg-card/)
  assert.match(panel, /h-8 border-b border-border bg-muted/)
  assert.match(panel, /divide-y divide-border/)
  assert.match(panel, /grid-cols-\[minmax\(0,1fr\)_112px_112px\]/)
  assert.match(panel, /t\('run\.accountMenu\.manage'\)/)
  assert.match(panel, /usageColumns\(/)
  // UsageRing is the shared donut only
  assert.doesNotMatch(read('../../components/UsageRing.vue'), /Teleport|pinned/)
  const keys = ['title', 'summaryTitle', 'col5h', 'colWeek', 'using', 'refreshAll', 'noData', 'refresh', 'windowOf', 'more']
  for (const lang of ['en', 'vi']) {
    const usage = json(`../../i18n/locales/${lang}/misc.json`).budget.usage
    for (const k of keys) assert.equal(typeof usage[k], 'string', `${lang} misc.budget.usage.${k}`)
  }
})
