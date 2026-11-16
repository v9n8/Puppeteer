// Unit tests for lib/projectSections.ts (project settings nav groups + `?section=` deep link).
// Run: node --test src/lib/__tests__/projectSections.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  PROJECT_GROUPS, PROJECT_SECTIONS, projectNav, sectionFromQuery, sectionGroup, tabSection, projectPageQuery,
  accountTypeFromQuery,
} from '../projectSections.ts'

const shape = (nav) => nav.map((g) => [g.id, g.sections.map((s) => s.id)])

// The flat tabs before the grouping: each must still be in the nav, under the same id.
const OLD_TABS = [
  'overview', 'skills', 'rules', 'mcp', 'deploy', 'github', 'board',
  'gitlab', 'aws', 'azure', 'claude', 'tools', 'conflicts',
]

test('every section belongs to a known group, ids are unique', () => {
  const groups = new Set(PROJECT_GROUPS.map((g) => g.id))
  for (const s of PROJECT_SECTIONS) assert.ok(groups.has(s.group), `${s.id} → ${s.group}`)
  assert.equal(new Set(PROJECT_SECTIONS.map((s) => s.id)).size, PROJECT_SECTIONS.length)
})

// UX-19: the Claude account tab became part of `session`; `styles` is new.
// UX-57: `skills` and `rules` became the read-only `files` list.
// V3-44: GitHub / GitLab / AWS / Azure became `accounts`; `mcp` moved to Connections.
const MOVED = {
  claude: 'session', skills: 'files', rules: 'files',
  github: 'accounts', gitlab: 'accounts', aws: 'accounts', azure: 'accounts',
}

test('no old tab is lost', () => {
  const now = PROJECT_SECTIONS.map((s) => s.id)
  for (const id of OLD_TABS) assert.ok(now.includes(MOVED[id] ?? id), id)
  assert.deepEqual(now.filter((id) => !OLD_TABS.includes(id)).sort(), ['accounts', 'files', 'flags', 'session', 'styles'])
})

test('projectNav: five groups in order, conflicts under Library when present', () => {
  assert.deepEqual(shape(projectNav(PROJECT_SECTIONS, true)), [
    ['overview', ['overview', 'board']],
    ['defaults', ['session', 'styles', 'flags']],
    ['connections', ['accounts', 'mcp']],
    ['library', ['files', 'conflicts']],
    ['servers', ['deploy', 'tools']],
  ])
})

test('projectNav: no conflicts → no Conflicts entry', () => {
  const library = projectNav(PROJECT_SECTIONS, false).find((g) => g.id === 'library')
  assert.deepEqual(library.sections.map((s) => s.id), ['files'])
})

test('projectNav: an empty group gets no heading, groups follow PROJECT_GROUPS order', () => {
  const sections = [
    { id: 'tools', group: 'servers' },
    { id: 'conflicts', group: 'library' },
    { id: 'overview', group: 'overview' },
  ]
  assert.deepEqual(shape(projectNav(sections, false)), [['overview', ['overview']], ['servers', ['tools']]])
  assert.deepEqual(projectNav(sections, true).map((g) => g.id), ['overview', 'library', 'servers'])
})

test('sectionFromQuery: every old tab id opens its section, anything else is ignored', () => {
  for (const id of OLD_TABS) assert.equal(sectionFromQuery(id), MOVED[id] ?? id)
  for (const id of ['session', 'styles', 'flags', 'files']) assert.equal(sectionFromQuery(id), id)
  for (const inherited of ['toString', 'constructor', '__proto__']) assert.equal(sectionFromQuery(inherited), null)
  for (const bad of [undefined, null, '', 'library', 'defaults', ['board'], 42]) {
    assert.equal(sectionFromQuery(bad), null, String(bad))
  }
})

test('sectionGroup: the tab each section sits in', () => {
  assert.equal(sectionGroup('overview'), 'overview')
  assert.equal(sectionGroup('board'), 'overview')
  assert.equal(sectionGroup('styles'), 'defaults')
  assert.equal(sectionGroup('accounts'), 'connections')
  assert.equal(sectionGroup('mcp'), 'connections')
  assert.equal(sectionGroup('conflicts'), 'library')
  assert.equal(sectionGroup('deploy'), 'servers')
})

test('tabSection: the requested section while the tab lists it, else the first', () => {
  assert.equal(tabSection('connections', 'mcp', false), 'mcp')
  assert.equal(tabSection('connections', null, false), 'accounts')
  // A section of another tab is not shown here.
  assert.equal(tabSection('connections', 'files', false), 'accounts')
  assert.equal(tabSection('defaults', 'styles', false), 'styles')
  // Conflicts only while there are some.
  assert.equal(tabSection('library', 'conflicts', true), 'conflicts')
  assert.equal(tabSection('library', 'conflicts', false), 'files')
  for (const g of PROJECT_GROUPS) assert.equal(sectionGroup(tabSection(g.id, null, false)), g.id)
})

test('projectPageQuery: the old settings link opens the same section on the Projects page', () => {
  assert.deepEqual(projectPageQuery('p1', 'board'), { id: 'p1', tab: 'overview', section: 'board' })
  assert.deepEqual(projectPageQuery('p1', 'claude'), { id: 'p1', tab: 'defaults', section: 'session' })
  assert.deepEqual(projectPageQuery('p1', 'rules'), { id: 'p1', tab: 'library', section: 'files' })
  assert.deepEqual(projectPageQuery('p1', 'tools'), { id: 'p1', tab: 'servers', section: 'tools' })
  assert.deepEqual(projectPageQuery('p1'), { id: 'p1' })
  assert.deepEqual(projectPageQuery('p1', 'nope'), { id: 'p1' })
  for (const id of OLD_TABS) {
    const q = projectPageQuery('p1', id)
    assert.equal(q.section, MOVED[id] ?? id)
    assert.equal(q.tab, sectionGroup(q.section))
  }
})

test('projectPageQuery: an old account section keeps its type as the list filter', () => {
  assert.deepEqual(projectPageQuery('p1', 'github'), { id: 'p1', tab: 'connections', section: 'accounts', type: 'github' })
  assert.deepEqual(projectPageQuery('p1', 'azure'), { id: 'p1', tab: 'connections', section: 'accounts', type: 'azure' })
  assert.deepEqual(projectPageQuery('p1', 'mcp'), { id: 'p1', tab: 'connections', section: 'mcp' })
})

test('accountTypeFromQuery: ?type= wins, else the old section id, else all', () => {
  assert.equal(accountTypeFromQuery('accounts', 'gitlab'), 'gitlab')
  assert.equal(accountTypeFromQuery('aws'), 'aws')
  assert.equal(accountTypeFromQuery('azure', 'github'), 'github')
  for (const bad of [undefined, null, 'google', 'mcp', 'toString', ['aws']]) assert.equal(accountTypeFromQuery('accounts', bad), null, String(bad))
  assert.equal(accountTypeFromQuery('accounts'), null)
})

test('every section and group has an en and vi label', () => {
  for (const lang of ['en', 'vi']) {
    const msgs = JSON.parse(readFileSync(new URL(`../../i18n/locales/${lang}/projectDetail.json`, import.meta.url), 'utf8'))
    for (const s of PROJECT_SECTIONS) assert.ok(msgs.sections[s.id], `${lang} sections.${s.id}`)
    for (const g of PROJECT_GROUPS) {
      const key = g.labelKey.replace(/^projectDetail\./, '').split('.')
      assert.ok(key.reduce((o, k) => o?.[k], msgs), `${lang} ${g.labelKey}`)
    }
  }
})
