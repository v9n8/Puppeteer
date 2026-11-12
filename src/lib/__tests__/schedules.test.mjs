// Unit tests for lib/schedules.ts (the Schedules screen's editor form).
// Run: node --test src/lib/__tests__/schedules.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  PERMISSION_MODES,
  emptyForm,
  formError,
  formFromSchedule,
  inputFromForm,
  timingFromForm,
  timingSummary,
  toggleWeekday,
} from '../schedules.ts'

const filled = (patch = {}) => ({ ...emptyForm('p1'), name: ' Morning PRs ', prompt: ' Review PRs ', ...patch })

const schedule = (timing, patch = {}) => ({
  id: 's1',
  project_id: 'p1',
  name: 'n',
  prompt: 'p',
  engine: 'codex',
  permission_mode: 'acceptEdits',
  timing,
  enabled: true,
  proposed: false,
  next_run_at: null,
  last_run_at: null,
  last_run_id: null,
  last_status: null,
  last_message: null,
  created_at: 'c',
  updated_at: 'u',
  ...patch,
})

test('a new form is a weekday-morning plan-mode schedule on the default engine', () => {
  const f = emptyForm('p1')
  assert.deepEqual(timingFromForm(f), { kind: 'daily', time: '09:00' })
  assert.equal(f.permission_mode, 'plan')
  assert.equal(inputFromForm(filled()).engine, null)
})

test('the input carries the timing of the chosen kind only, in the Rust shape', () => {
  assert.deepEqual(inputFromForm(filled({ kind: 'interval', every_minutes: 30 })), {
    id: null,
    project_id: 'p1',
    name: 'Morning PRs',
    prompt: 'Review PRs',
    engine: null,
    permission_mode: 'plan',
    timing: { kind: 'interval', every_minutes: 30 },
  })
  assert.deepEqual(timingFromForm(filled({ kind: 'weekly', weekdays: [5, 1, 5], time: '07:30' })), {
    kind: 'weekly',
    weekdays: [1, 5],
    time: '07:30',
  })
  assert.deepEqual(timingFromForm(filled({ kind: 'once', once_at: '2026-11-09T07:15' })), { kind: 'once', at: '2026-11-09T07:15' })
})

test('a stored schedule round-trips through the form', () => {
  for (const timing of [
    { kind: 'interval', every_minutes: 45 },
    { kind: 'daily', time: '08:30' },
    { kind: 'weekly', weekdays: [0, 6], time: '10:00' },
    { kind: 'once', at: '2026-12-01T06:00' },
  ]) {
    const f = formFromSchedule(schedule(timing))
    assert.deepEqual(inputFromForm(f), {
      id: 's1',
      project_id: 'p1',
      name: 'n',
      prompt: 'p',
      engine: 'codex',
      permission_mode: 'acceptEdits',
      timing,
    })
  }
  // An unreadable timing opens on the default one.
  assert.deepEqual(timingFromForm(formFromSchedule(schedule(null))), { kind: 'daily', time: '09:00' })
})

test('formError names the first field the backend would refuse', () => {
  assert.equal(formError(filled()), null)
  const cases = [
    [{ project_id: '' }, 'project'],
    [{ name: '   ' }, 'name'],
    [{ name: 'x'.repeat(201) }, 'name'],
    [{ prompt: '' }, 'prompt'],
    [{ kind: 'interval', every_minutes: 0 }, 'every_minutes'],
    [{ kind: 'interval', every_minutes: 1.5 }, 'every_minutes'],
    [{ kind: 'interval', every_minutes: 7 * 24 * 60 + 1 }, 'every_minutes'],
    [{ kind: 'daily', time: '8:30' }, 'time'],
    [{ kind: 'weekly', time: '24:00' }, 'time'],
    [{ kind: 'weekly', weekdays: [] }, 'weekdays'],
    [{ kind: 'once', once_at: '' }, 'once_at'],
    [{ kind: 'once', once_at: '2026-11-09T07:15:00' }, 'once_at'],
  ]
  for (const [patch, want] of cases) assert.equal(formError(filled(patch)), want, JSON.stringify(patch))
  // Fields of another kind do not count.
  assert.equal(formError(filled({ kind: 'interval', time: 'bad', once_at: '' })), null)
})

test('toggleWeekday adds in order and removes', () => {
  assert.deepEqual(toggleWeekday([1, 5], 3), [1, 3, 5])
  assert.deepEqual(toggleWeekday([1, 3, 5], 3), [1, 5])
  assert.deepEqual(toggleWeekday([], 0), [0])
})

test('timingSummary picks a key per kind', () => {
  assert.deepEqual(timingSummary({ kind: 'interval', every_minutes: 120 }), { key: 'everyHours', params: { n: 2 } })
  assert.deepEqual(timingSummary({ kind: 'interval', every_minutes: 90 }), { key: 'everyMinutes', params: { n: 90 } })
  assert.deepEqual(timingSummary({ kind: 'daily', time: '08:30' }), { key: 'daily', params: { time: '08:30' } })
  assert.deepEqual(timingSummary({ kind: 'weekly', weekdays: [1, 3], time: '08:30' }), {
    key: 'weekly',
    params: { time: '08:30', days: [1, 3] },
  })
  assert.deepEqual(timingSummary({ kind: 'once', at: '2026-11-09T07:15' }), { key: 'once', params: { at: '2026-11-09 07:15' } })
  assert.deepEqual(timingSummary(null), { key: 'invalid', params: {} })
})

test('every schedule mode, Auto included, has a label and a hint in en and vi', () => {
  assert.ok(PERMISSION_MODES.includes('auto'))
  for (const lang of ['en', 'vi']) {
    const msgs = JSON.parse(readFileSync(new URL(`../../i18n/locales/${lang}/schedules.json`, import.meta.url), 'utf8'))
    for (const mode of PERMISSION_MODES) {
      assert.ok(msgs.mode[mode]?.trim(), `${lang}: ${mode} label`)
      assert.ok(msgs.modeHint[mode]?.trim(), `${lang}: ${mode} hint`)
    }
  }
})
