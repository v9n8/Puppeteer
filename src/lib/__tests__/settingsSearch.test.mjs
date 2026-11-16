// Unit tests for lib/settingsSearch.ts + lib/settingsSearchRows.ts (the Settings search).
// Run: node --test src/lib/__tests__/settingsSearch.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { SETTINGS_SECTIONS } from '../settingsSections.ts'
import { SETTINGS_SEARCH_ROWS } from '../settingsSearchRows.ts'
import {
  MAX_SEARCH_RESULTS, TAB_LABEL_KEY, resultPath, searchSettings, settingsSearchIndex,
} from '../settingsSearch.ts'

const LOCALES = new URL('../../i18n/locales/', import.meta.url)
const messages = {}
for (const lang of ['en', 'vi']) {
  messages[lang] = {}
  for (const f of readdirSync(new URL(`${lang}/`, LOCALES))) {
    messages[lang][f.replace(/\.json$/, '')] = JSON.parse(readFileSync(new URL(`${lang}/${f}`, LOCALES), 'utf8'))
  }
}
/** The message of a full key (`<file>.<path…>`), or undefined. */
const lookup = (lang, key) => {
  const [ns, ...path] = key.split('.')
  return path.reduce((o, k) => (o && typeof o === 'object' ? o[k] : undefined), messages[lang][ns])
}
const tFor = (lang) => (key) => lookup(lang, key) ?? key
const t = tFor('vi')
const tEn = tFor('en')
const index = settingsSearchIndex(SETTINGS_SECTIONS, SETTINGS_SEARCH_ROWS, t, tEn)
const prop = (id, effective) => ({
  id, stored: null, effective, locked: false, default: effective, degraded: [], reason_key: null,
})
const treeAll = (on) => [
  { module: 'productivity', enabled: on, locked: false, props: [prop('productivity.focus', on)] },
  { module: 'app', enabled: true, locked: false, props: [prop('app.command_palette', on)] },
  { module: 'infra_access', enabled: on, locked: false, props: [prop('infra_access.aws', on), prop('infra_access.azure', on)] },
  { module: 'integrations', enabled: on, locked: false, props: [prop('integrations.google', on)] },
]
const find = (q, tree = treeAll(true)) => searchSettings(index, q, tree)
const ids = (list) => list.map((e) => e.id)

// ── Search behaviour ────────────────────────────────────────────────────────

test('"font" finds the font rows of General', () => {
  const hit = find('font')
  assert.ok(hit.length > 0)
  assert.ok(ids(hit).includes('general-font'))
  assert.ok(hit.filter((e) => e.section === 'general').length >= 4)
})

test('accents are ignored: "phong nen" / "phong chu" match Vietnamese labels and keywords', () => {
  assert.ok(find('phong chu').length > 0)
  assert.ok(ids(find('phim tat')).includes('shortcuts-keymap'))
})

test('English labels match while the app is in Vietnamese', () => {
  assert.ok(ids(find('color theme')).includes('general-colorTheme'))
})

test('every word must be present, any order; case is ignored', () => {
  assert.ok(ids(find('THEME color')).includes('general-colorTheme'))
  assert.deepEqual(find('font zzzzqq'), [])
})

test('empty query and no match give nothing', () => {
  assert.deepEqual(find(''), [])
  assert.deepEqual(find('   '), [])
  assert.deepEqual(find('xyz'), [])
})

test('a row name ranks before a keyword-only match, results are capped', () => {
  const hit = find('model')
  assert.ok(hit.length <= MAX_SEARCH_RESULTS)
  const firstKeywordOnly = hit.findIndex((e) => !e.labelFolded.includes('model'))
  const lastNameMatch = hit.map((e) => e.labelFolded.includes('model')).lastIndexOf(true)
  if (firstKeywordOnly !== -1) assert.ok(lastNameMatch < firstKeywordOnly)
})

test('a section switched off by its prop gives no result (Focus, Shortcuts)', () => {
  const off = treeAll(false)
  assert.ok(find('focus').some((e) => e.section === 'focus'))
  assert.ok(!find('focus', off).some((e) => e.section === 'focus'))
  assert.ok(find('phim').some((e) => e.section === 'shortcuts'))
  assert.ok(!find('phim', off).some((e) => e.section === 'shortcuts'))
})

test('resultPath: Group › Section, plus the tab of a row inside a tab', () => {
  const quota = index.find((e) => e.id === 'limits-block5h')
  assert.equal(resultPath(quota, t), `${t('settings.groups.ai')} › ${t('settings.sections.limits')} › ${t('settings.sessions.quotaTitle')}`)
  const font = index.find((e) => e.id === 'general-font')
  assert.equal(resultPath(font, t).split(' › ').length, 2)
})

test('search terms of the acceptance list all find something', () => {
  const words = [
    'font', 'theme', 'phím', 'shortcut', 'ghim', 'MCP', 'flag', 'model', 'quota', 'giới hạn', 'relay',
    'xác thực tất cả', 'phiên bản', 'version', 'cập nhật', 'có gì mới', 'sao lưu', 'backup', 'memory', 'dịch',
  ]
  for (const w of words) assert.ok(find(w).length > 0, `no result for "${w}"`)
})

// ── The index is complete and true ──────────────────────────────────────────

test('every section is found by its own name, and has at least one declared row', () => {
  for (const s of SETTINGS_SECTIONS) {
    assert.ok(index.some((e) => e.isSection && e.section === s.id), `${s.id}: no entry by name`)
    assert.ok((SETTINGS_SEARCH_ROWS[s.id] ?? []).length >= 1, `${s.id}: no row declared`)
    assert.ok(find(t(s.labelKey)).some((e) => e.section === s.id), `${s.id}: not found by "${t(s.labelKey)}"`)
  }
})

test('no unknown section in the rows, and row ids are unique', () => {
  const known = new Set(SETTINGS_SECTIONS.map((s) => s.id))
  for (const k of Object.keys(SETTINGS_SEARCH_ROWS)) assert.ok(known.has(k), `rows for unknown section ${k}`)
  const all = Object.values(SETTINGS_SEARCH_ROWS).flat().map((r) => r.id)
  assert.equal(new Set(all).size, all.length, 'duplicate row id')
})

test('every labelKey / keywordsKey / tab label exists in en and vi', () => {
  const keys = new Set(SETTINGS_SECTIONS.map((s) => s.labelKey))
  for (const g of ['general', 'ai', 'connections', 'system']) keys.add(`settings.groups.${g}`)
  for (const r of Object.values(SETTINGS_SEARCH_ROWS).flat()) {
    keys.add(r.labelKey)
    if (r.keywordsKey) keys.add(r.keywordsKey)
  }
  for (const tabs of Object.values(TAB_LABEL_KEY)) for (const k of Object.values(tabs)) keys.add(k)
  for (const k of ['placeholder', 'label', 'count', 'footer', 'none', 'noneHint', 'clear']) keys.add(`settings.search.${k}`)
  for (const lang of ['en', 'vi']) {
    for (const k of keys) assert.equal(typeof lookup(lang, k), 'string', `${lang}: missing ${k}`)
  }
})

test('a row inside a tab names a tab its section really has', () => {
  for (const [section, rows] of Object.entries(SETTINGS_SEARCH_ROWS)) {
    for (const r of rows.filter((x) => x.tab)) {
      assert.ok(TAB_LABEL_KEY[section]?.[r.tab], `${r.id}: tab ${r.tab} is not declared for ${section}`)
    }
  }
})

function vueFiles(dir) {
  const out = []
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) out.push(...vueFiles(p))
    else if (name.endsWith('.vue')) out.push(p)
  }
  return out
}

test('every row id is carried by a screen (no ghost rows)', () => {
  const sources = vueFiles(new URL('../../', import.meta.url).pathname)
    .map((p) => readFileSync(p, 'utf8'))
    .filter((s) => /settings-id|search-id/.test(s))
  for (const r of Object.values(SETTINGS_SEARCH_ROWS).flat()) {
    const direct = new RegExp(`(data-settings-id|search-id)="${r.id}"`)
    const quoted = new RegExp(`['"]${r.id}['"]`)
    assert.ok(sources.some((s) => direct.test(s) || (/data-settings-id/.test(s) && quoted.test(s))), `${r.id}: no data-settings-id / search-id in any .vue`)
  }
})
