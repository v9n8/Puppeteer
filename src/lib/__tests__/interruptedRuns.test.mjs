// Unit tests for lib/interruptedRuns.ts (the "sessions cut off when the app
// closed" banner and the History label, HD-06).
// Run: node --test src/lib/__tests__/interruptedRuns.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { applyOutcomes, rowIds, runStatusKey, toggleSelected } from '../interruptedRuns.ts'

const row = (run_id, worker_count = 0) => ({
  run_id,
  project_id: 'p',
  project_name: 'proj',
  title: `title ${run_id}`,
  engine: 'claude',
  worker_count,
  permission_mode: 'default',
})

// A Claude run, a Codex run and a conductor whose 2 workers fold into its row.
const rows = [row('claude'), { ...row('codex'), engine: 'codex' }, row('conductor', 2)]

test('Resume all and Dismiss act on every row, in banner order', () => {
  assert.deepEqual(rowIds(rows), ['claude', 'codex', 'conductor'])
  assert.deepEqual(rowIds([]), [])
})

test('the picker ticks and unticks one row, keeping banner order', () => {
  let sel = toggleSelected(rows, [], 'conductor')
  assert.deepEqual(sel, ['conductor'])
  sel = toggleSelected(rows, sel, 'claude')
  assert.deepEqual(sel, ['claude', 'conductor'])
  sel = toggleSelected(rows, sel, 'conductor')
  assert.deepEqual(sel, ['claude'])
})

test('the picker drops ids no longer on the banner', () => {
  assert.deepEqual(toggleSelected(rows, ['gone', 'codex'], 'claude'), ['claude', 'codex'])
})

test('resuming all: resumed rows leave, a failed one stays with its own error', () => {
  const after = applyOutcomes({ rows, errors: {} }, [
    { run_id: 'claude', ok: true },
    { run_id: 'codex', ok: false, error: 'Usage is over the quota block threshold' },
    { run_id: 'conductor', ok: true },
  ])
  assert.deepEqual(rowIds(after.rows), ['codex'])
  assert.deepEqual(after.errors, { codex: 'Usage is over the quota block threshold' })
})

test('resuming one picked row leaves the others on the banner untouched', () => {
  const before = { rows, errors: { codex: 'earlier error' } }
  const after = applyOutcomes(before, [{ run_id: 'claude', ok: true }])
  assert.deepEqual(rowIds(after.rows), ['codex', 'conductor'])
  assert.deepEqual(after.errors, { codex: 'earlier error' })
  // The input state is not changed in place.
  assert.equal(before.rows.length, 3)
})

test('a retry that works clears the row and its old error', () => {
  const after = applyOutcomes({ rows: [rows[1]], errors: { codex: 'quota' } }, [{ run_id: 'codex', ok: true }])
  assert.deepEqual(after, { rows: [], errors: {} })
})

test('a failure without a message still stays on the banner', () => {
  const after = applyOutcomes({ rows, errors: {} }, [{ run_id: 'claude', ok: false }])
  assert.equal(after.rows.length, 3)
  assert.deepEqual(after.errors, { claude: '' })
})

test('History tells a cut-off or quota-stopped run from one the user stopped', () => {
  const run = (status, end_reason, run_type = 'session') => ({ status, end_reason, run_type })
  assert.equal(runStatusKey(run('cancelled', 'app_closed')), 'common.status.appClosed')
  assert.equal(runStatusKey(run('cancelled', 'quota')), 'common.status.quotaStopped')
  assert.equal(runStatusKey(run('cancelled', null)), 'common.status.cancelled')
  assert.equal(runStatusKey({ status: 'cancelled', run_type: 'session' }), 'common.status.cancelled')
  // A run that ran again after the mark: its status wins.
  assert.equal(runStatusKey(run('running', 'app_closed')), 'common.status.running')
  assert.equal(runStatusKey(run('done', 'app_closed')), 'common.status.done')
  assert.equal(runStatusKey(run('fetched', null)), 'common.status.draft')
  assert.equal(runStatusKey(run('fetched', null, 'analyze_issue')), 'common.status.fetched')
})

test('every History label and banner string exists in en and vi', () => {
  const load = (lang, ns) =>
    JSON.parse(readFileSync(new URL(`../../i18n/locales/${lang}/${ns}.json`, import.meta.url), 'utf8'))
  const keys = (node, prefix = '') =>
    Object.entries(node).flatMap(([k, v]) =>
      v && typeof v === 'object' ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
    )
  for (const lang of ['en', 'vi']) {
    const status = load(lang, 'common').status
    assert.ok(status.appClosed && status.quotaStopped, lang)
  }
  assert.deepEqual(keys(load('vi', 'interruptedRuns')), keys(load('en', 'interruptedRuns')))
  assert.equal(load('en', 'common').status.appClosed, 'Interrupted when the app closed')
  assert.equal(load('vi', 'common').status.appClosed, 'Bị ngắt khi app tắt')
})

// HD-AC-06: only a click resumes a cut-off run. The banner is the only caller
// of the command, its call sits in one function, and only the two Resume
// buttons reach that function (no mount hook, watcher or timer).
test('nothing but the Resume buttons calls resume_interrupted_runs', () => {
  const srcDir = fileURLToPath(new URL('../../', import.meta.url))
  const callers = []
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name)
      if (statSync(path).isDirectory()) {
        if (name !== '__tests__') walk(path)
      } else if (/\.(ts|vue|js|mjs)$/.test(name) && readFileSync(path, 'utf8').includes('resume_interrupted_runs')) {
        callers.push(path.slice(srcDir.length))
      }
    }
  }
  walk(srcDir)
  assert.deepEqual(callers.sort(), ['components/InterruptedRunsBanner.vue', 'lib/interruptedRuns.ts'])
  // lib/interruptedRuns.ts only names it in a comment.
  assert.doesNotMatch(readFileSync(new URL('../interruptedRuns.ts', import.meta.url), 'utf8'), /invoke/)

  const banner = readFileSync(new URL('../../components/InterruptedRunsBanner.vue', import.meta.url), 'utf8')
  assert.equal(banner.match(/'resume_interrupted_runs'/g).length, 1)
  assert.match(banner, /async function resume\(ids: string\[\]\) \{[^}]*'resume_interrupted_runs'/)
  const script = banner.slice(0, banner.indexOf('</script>'))
  const template = banner.slice(banner.indexOf('<template>'))
  // `resume(` is called from resumeAll and resumePicked only.
  assert.equal(script.match(/[^\w]resume\(/g).length, 3) // the definition + 2 calls
  assert.match(script, /function resumeAll\(\) \{\s*void resume\(/)
  assert.match(script, /async function resumePicked\(\) \{[^}]*await resume\(ids\)/)
  for (const fn of ['resumeAll', 'resumePicked']) {
    assert.equal(script.match(new RegExp(`\\b${fn}\\b`, 'g')).length, 1, `${fn} is only defined in the script`)
    assert.deepEqual(template.match(new RegExp(`@click="${fn}"`, 'g')), [`@click="${fn}"`], `${fn} is a button's click`)
  }
  assert.doesNotMatch(script, /setTimeout|setInterval|watch\(/)
})
