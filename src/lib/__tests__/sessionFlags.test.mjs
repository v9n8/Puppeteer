// Unit tests for lib/sessionFlags.ts (V3-18).
// Run: node --test src/lib/__tests__/sessionFlags.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  DEFAULT_FLAG_KEYS, FLAG_NAME_MAX, flagById, flagLabel, flagMenuGroups, flagNameError, missingDefaultFlags, usableFlagIds,
} from '../sessionFlags.ts'

const flag = (id, extra = {}) => ({
  id, project_id: null, name: id, color: 'info', position: 0, builtin_key: null, stock_name: false, hidden: false, run_count: 0,
  ...extra,
})
const t = (key) => `<${key}>`

test('the defaults and the name limit match the backend', () => {
  assert.deepEqual([...DEFAULT_FLAG_KEYS], ['needs_review', 'in_progress', 'awaiting_review', 'settled'])
  assert.equal(FLAG_NAME_MAX, 24)
  const source = readFileSync(new URL('../../../src-tauri/src/commands/session_flags.rs', import.meta.url), 'utf8')
  assert.match(source, /pub const NAME_MAX_CHARS: usize = 24;/)
  for (const key of DEFAULT_FLAG_KEYS) assert.ok(source.includes(`key: "${key}"`), key)
})

test('every default has a translated name in both languages', () => {
  for (const lang of ['en', 'vi']) {
    const messages = JSON.parse(readFileSync(new URL(`../../i18n/locales/${lang}/sessionFlags.json`, import.meta.url), 'utf8'))
    for (const key of DEFAULT_FLAG_KEYS) assert.ok(messages.builtin[key], `${lang} ${key}`)
  }
})

test('a default shows its translated name until the user renames it', () => {
  const def = flag('settled', { name: 'Settled', builtin_key: 'settled', stock_name: true })
  assert.equal(flagLabel(def, t), '<sessionFlags.builtin.settled>')
  assert.equal(flagLabel({ ...def, name: 'Done', stock_name: false }, t), 'Done')
  assert.equal(flagLabel(flag('Blocked'), t), 'Blocked')
})

test('the menu offers shared then own flags, without the hidden ones', () => {
  const flags = [flag('a'), flag('b', { hidden: true }), flag('c'), flag('own', { project_id: 'p1' })]
  const groups = flagMenuGroups(flags)
  assert.deepEqual(groups.shared.map((f) => f.id), ['a', 'c'])
  assert.deepEqual(groups.project.map((f) => f.id), ['own'])
  assert.deepEqual([...usableFlagIds(flags)], ['a', 'c', 'own'])
  assert.deepEqual(flagMenuGroups([]), { shared: [], project: [] })
})

test('restore is needed only for the deleted defaults', () => {
  const all = DEFAULT_FLAG_KEYS.map((k) => flag(k, { builtin_key: k }))
  assert.deepEqual(missingDefaultFlags(all), [])
  assert.deepEqual(missingDefaultFlags([all[0], all[3], flag('mine')]), ['in_progress', 'awaiting_review'])
  assert.deepEqual(missingDefaultFlags([]), [...DEFAULT_FLAG_KEYS])
})

test('a flag is found by id; no id or a deleted one finds nothing', () => {
  const flags = [flag('a'), flag('b')]
  assert.equal(flagById(flags, 'b')?.id, 'b')
  assert.equal(flagById(flags, 'gone'), undefined)
  assert.equal(flagById(flags, null), undefined)
  assert.equal(flagById(flags, undefined), undefined)
})

test('a name is checked like the backend: trimmed, 1 to 24 characters, unique in its scope', () => {
  const scope = [flag('a', { name: 'In progress' }), flag('b', { name: 'Blocked' })]
  assert.equal(flagNameError('  ', scope), 'sessionFlags.errors.nameRequired')
  assert.equal(flagNameError('x'.repeat(FLAG_NAME_MAX + 1), scope), 'sessionFlags.errors.nameTooLong')
  // Counted in characters, not UTF-16 units.
  assert.equal(flagNameError('😀'.repeat(FLAG_NAME_MAX), scope), null)
  assert.equal(flagNameError(' in PROGRESS ', scope), 'sessionFlags.errors.nameTaken')
  // Renaming a flag to its own name in another case is fine.
  assert.equal(flagNameError('IN PROGRESS', scope, 'a'), null)
  assert.equal(flagNameError('Waiting on client', scope), null)
})
