// Unit tests for lib/runNotificationText.ts (HD-09: desktop OS notifications
// about runs — kind, switch, translated text).
// Run: node --test src/lib/__tests__/runNotificationText.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  QUOTA_END_REASON,
  allowsRunNotification,
  finishedRunKind,
  notifySwitch,
  runNotificationText,
} from '../runNotificationText.ts'
import { parseDesktopNotify } from '../desktopNotify.ts'

const messages = (lang) =>
  JSON.parse(readFileSync(new URL(`../../i18n/locales/${lang}/runNotify.json`, import.meta.url), 'utf8'))

/** A `t` over the real locale file, with vue-i18n's `{name}` interpolation. */
function translator(lang) {
  const root = { runNotify: messages(lang) }
  return (key, params = {}) => {
    const msg = key.split('.').reduce((node, k) => node?.[k], root)
    assert.equal(typeof msg, 'string', `${lang}: missing ${key}`)
    return msg.replace(/\{(\w+)\}/g, (_, name) => String(params[name]))
  }
}

const KINDS = ['needs_you', 'done', 'failed', 'stopped_quota']
const ENGINES = ['claude', 'codex']
const LANGS = ['en', 'vi']
const QUESTION = { tool_name: 'AskUserQuestion' }
const PLAN = { tool_name: 'ExitPlanMode' }

function text(lang, kind, engine, extra = {}) {
  return runNotificationText({ kind, engine, project: 'Shop', request: QUESTION, t: translator(lang), ...extra })
}

test('finished turns: done, failed, a quota stop; a turn the user stopped raises nothing', () => {
  assert.equal(finishedRunKind('done'), 'done')
  assert.equal(finishedRunKind('failed'), 'failed')
  assert.equal(finishedRunKind('cancelled', QUOTA_END_REASON), 'stopped_quota')
  assert.equal(finishedRunKind('cancelled'), null)
  assert.equal(finishedRunKind('cancelled', null), null)
  assert.equal(finishedRunKind('cancelled', 'app_closed'), null)
})

test('each kind follows its switch; a quota stop follows "Failed or stopped"', () => {
  assert.deepEqual(KINDS.map(notifySwitch), ['needs_you', 'done', 'failed', 'failed'])
  const doneOff = parseDesktopNotify('{"done":false}')
  assert.equal(allowsRunNotification(doneOff, 'done'), false)
  for (const k of ['needs_you', 'failed', 'stopped_quota']) assert.equal(allowsRunNotification(doneOff, k), true, k)
  const needsYouOff = parseDesktopNotify('{"needs_you":false}')
  assert.equal(allowsRunNotification(needsYouOff, 'needs_you'), false)
  const failedOff = parseDesktopNotify('{"failed":false}')
  assert.equal(allowsRunNotification(failedOff, 'failed'), false)
  assert.equal(allowsRunNotification(failedOff, 'stopped_quota'), false)
  assert.equal(allowsRunNotification(failedOff, 'done'), true)
  for (const k of KINDS) assert.equal(allowsRunNotification(parseDesktopNotify(''), k), true, k)
})

test('every kind × engine × language has a title and a body; Codex never says Claude', () => {
  for (const lang of LANGS) {
    for (const engine of ENGINES) {
      for (const kind of KINDS) {
        for (const request of [QUESTION, PLAN]) {
          const { title, body } = text(lang, kind, engine, { request })
          assert.ok(title.trim() && body.trim(), `${lang}/${engine}/${kind}`)
          assert.ok(title.includes('Shop'), `${lang}/${engine}/${kind}: project in title`)
          if (engine === 'codex') assert.ok(!`${title} ${body}`.includes('Claude'), `${lang}/${kind}: ${title} / ${body}`)
        }
      }
    }
  }
})

test('the engine is named from the run, not hard-coded', () => {
  assert.equal(text('en', 'needs_you', 'codex').body, 'Codex is asking you a question')
  assert.equal(text('en', 'needs_you', 'claude').body, 'Claude is asking you a question')
  assert.equal(text('en', 'done', 'codex').body, 'Codex finished responding')
  assert.equal(text('en', 'needs_you', 'codex', { request: PLAN }).body, 'Codex finished planning — review the plan')
  // Unknown engine: no engine named at all.
  const unknown = text('en', 'done', null)
  assert.ok(!unknown.body.includes('Claude') && !unknown.body.includes('Codex'), unknown.body)
})

test('a Codex run finishing in vi reads in Vietnamese (HD-AC-21)', () => {
  const { title, body } = text('vi', 'done', 'codex')
  assert.equal(title, 'Shop — Session đã xong')
  assert.equal(body, 'Codex đã trả lời xong')
})

test('the en wording stays close to the old one', () => {
  assert.deepEqual(text('en', 'needs_you', 'claude'), { title: 'Shop needs permission', body: 'Claude is asking you a question' })
  assert.equal(text('en', 'needs_you', 'claude', { reminder: true }).title, 'Shop is still waiting for permission')
  assert.equal(text('en', 'done', 'claude').title, 'Shop — session finished')
  assert.equal(text('en', 'failed', 'claude').title, 'Shop — session failed')
})

test('a finished turn shows the session title when there is one', () => {
  assert.equal(text('en', 'done', 'codex', { title: '  Fix login  ' }).body, 'Fix login')
  assert.equal(text('vi', 'failed', 'codex', { title: 'Fix login' }).body, 'Fix login')
})

test('a request shows the bridge sentence, else its name, else the tool', () => {
  const t = translator('en')
  const body = (request) => runNotificationText({ kind: 'needs_you', engine: 'codex', project: 'P', request, t }).body
  assert.equal(body({ tool_name: 'Edit', title: 'Wants to edit a.ts', display_name: 'Edit file' }), 'Wants to edit a.ts')
  assert.equal(body({ tool_name: 'Edit', display_name: 'Edit file' }), 'Edit file')
  assert.equal(body({ tool_name: 'Bash' }), 'Wants to run Bash')
  assert.equal(runNotificationText({ kind: 'needs_you', engine: 'codex', project: 'P', request: { tool_name: 'Bash' }, t: translator('vi') }).body, 'Muốn chạy Bash')
})

test('a quota stop: "session stopped", the translated reason, no %', () => {
  for (const lang of LANGS) {
    const { title, body } = text(lang, 'stopped_quota', 'claude', { title: 'Fix login' })
    assert.ok(!body.includes('%'), body)
    assert.equal(title, translator(lang)('runNotify.title.stopped', { project: 'Shop' }))
    assert.equal(body, translator(lang)('runNotify.body.stoppedQuota'))
  }
  assert.deepEqual(text('en', 'stopped_quota', 'codex'), {
    title: 'Shop — session stopped',
    body: 'Session stopped: quota limit reached',
  })
  assert.equal(text('vi', 'stopped_quota', 'codex').body, 'Session bị dừng: đã chạm giới hạn quota')
})

test('a turn a limit stopped says why (run:done reason)', () => {
  const reason = 'Stopped: turn limit reached (200 model calls)'
  assert.deepEqual(text('en', 'failed', 'codex', { reason, title: 'Nightly' }), { title: 'Shop — session stopped', body: reason })
  assert.equal(text('vi', 'failed', 'codex', { reason }).title, 'Shop — Session bị dừng')
})

test('en and vi have the same keys', () => {
  const keys = (node, prefix = '') =>
    Object.entries(node).flatMap(([k, v]) => (typeof v === 'object' ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`]))
  assert.deepEqual(keys(messages('vi')).sort(), keys(messages('en')).sort())
})
