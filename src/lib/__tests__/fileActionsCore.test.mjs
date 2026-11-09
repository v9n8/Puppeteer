// Unit tests for lib/fileActionsCore.ts ("Reveal in folder").
// Run: node --test src/lib/__tests__/fileActionsCore.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { revealIn } from '../fileActionsCore.ts'

function spyOpener() {
  const calls = []
  return {
    calls,
    openPath: async (path) => { calls.push(['openPath', path]) },
    revealItemInDir: async (path) => { calls.push(['revealItemInDir', path]) },
  }
}

test('revealIn: a .app bundle (a folder on macOS) is revealed, never opened', async () => {
  for (const abs of ['/p/Docs.app', '/p/Docs.APP/', '/p/run.command', '/p/x.workflow']) {
    const opener = spyOpener()
    await revealIn(opener, abs)
    assert.deepEqual(opener.calls, [['revealItemInDir', abs]], abs)
  }
})

test('revealIn: a plain folder is revealed too', async () => {
  const opener = spyOpener()
  await revealIn(opener, '/p/src')
  assert.deepEqual(opener.calls, [['revealItemInDir', '/p/src']])
})

test('revealIn: a file is revealed inside its folder', async () => {
  const opener = spyOpener()
  await revealIn(opener, '/p/src/a.ts')
  assert.deepEqual(opener.calls, [['revealItemInDir', '/p/src/a.ts']])
})
