// RT-02 (#17): a slash command from the session UI ("Compact context") goes out
// on its own, never mixed with the composer draft.
// Run: node --test src/lib/__tests__/slashCommand.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { slashCommandAction, slashCommandMessage } from '../slashCommand.ts'
import { composeWithQuotes } from '../quote.ts'

test('slashCommandAction: a finished or idle session sends it now', () => {
  for (const status of ['completed', 'failed', 'cancelled', 'idle', 'interrupted']) {
    assert.equal(slashCommandAction(status, false), 'send', status)
  }
})

test('slashCommandAction: a running session queues it behind the turn', () => {
  assert.equal(slashCommandAction('running', false), 'queue')
})

test('slashCommandAction: editing a message or a never-started session blocks it', () => {
  assert.equal(slashCommandAction('completed', true), 'blocked')
  assert.equal(slashCommandAction('running', true), 'blocked')
  assert.equal(slashCommandAction('fetched', false), 'blocked')
})

test('slashCommandMessage: only the command, no quotes, images or files', () => {
  const msg = slashCommandMessage('/compact')
  assert.deepEqual(msg, { text: '/compact', images: [], filePaths: [], quotes: [] })
  // So the prompt the model gets starts with the command (was "> quote\n\n…/compact").
  assert.equal(composeWithQuotes(msg.quotes, msg.text), '/compact')
})

test('sendSlashCommand leaves the draft alone and does not reuse the primary action', () => {
  const src = readFileSync(new URL('../../components/sessions/useRunComposer.ts', import.meta.url), 'utf8')
  const start = src.indexOf('function sendSlashCommand(')
  assert.ok(start > 0)
  const body = src.slice(start, src.indexOf('\n  }\n', start))
  assert.doesNotMatch(body, /followUpInput\.value\s*=/)
  assert.doesNotMatch(body, /draftQuotes\.value\s*=/)
  assert.doesNotMatch(body, /handlePrimaryAction|handleSendFollowUp|handleQueue\(|takePending/)
  assert.match(body, /slashCommandAction\(/)
})
