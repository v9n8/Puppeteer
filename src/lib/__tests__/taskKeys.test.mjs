import test from 'node:test'
import assert from 'node:assert/strict'
import { moveDueDate, nextSelection, taskKeyAction, weekNeighbour } from '../taskKeys.ts'

const ev = (key, extra = {}) => ({
  key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, isComposing: false, target: { tagName: 'DIV' }, ...extra,
})

test('keys map to actions', () => {
  assert.equal(taskKeyAction(ev('ArrowDown')), 'next')
  assert.equal(taskKeyAction(ev('ArrowUp')), 'prev')
  assert.equal(taskKeyAction(ev(' ')), 'done')
  assert.equal(taskKeyAction(ev('e')), 'edit')
  assert.equal(taskKeyAction(ev('s')), 'snooze')
  assert.equal(taskKeyAction(ev('Enter')), 'open')
  assert.equal(taskKeyAction(ev('x')), null)
})

test('left / right only in the week', () => {
  assert.equal(taskKeyAction(ev('ArrowLeft')), null)
  assert.equal(taskKeyAction(ev('ArrowRight'), { week: false }), null)
  assert.equal(taskKeyAction(ev('ArrowLeft'), { week: true }), 'left')
  assert.equal(taskKeyAction(ev('ArrowRight'), { week: true }), 'right')
})

test('keys are skipped while composing, with a modifier, or in a field', () => {
  assert.equal(taskKeyAction(ev('e', { isComposing: true })), null)
  assert.equal(taskKeyAction(ev('e', { keyCode: 229 })), null)
  for (const m of ['ctrlKey', 'metaKey', 'altKey', 'shiftKey']) assert.equal(taskKeyAction(ev('s', { [m]: true })), null)
  for (const tagName of ['INPUT', 'TEXTAREA', 'SELECT']) assert.equal(taskKeyAction(ev(' ', { target: { tagName } })), null)
  assert.equal(taskKeyAction(ev('e', { target: { tagName: 'DIV', isContentEditable: true } })), null)
})

test('a button keeps Space / Enter unless it is a list row', () => {
  assert.equal(taskKeyAction(ev(' ', { target: { tagName: 'BUTTON', dataset: {} } })), null)
  assert.equal(taskKeyAction(ev('Enter', { target: { tagName: 'A', dataset: {} } })), null)
  assert.equal(taskKeyAction(ev(' ', { target: { tagName: 'BUTTON', dataset: { taskRow: 't1' } } })), 'done')
  assert.equal(taskKeyAction(ev('Enter', { target: { tagName: 'DIV', dataset: {} } })), 'open')
})

test('nextSelection steps, stops at the ends and starts at the first / last', () => {
  const ids = ['a', 'b', 'c']
  assert.equal(nextSelection(ids, 'a', 'next'), 'b')
  assert.equal(nextSelection(ids, 'c', 'next'), 'c')
  assert.equal(nextSelection(ids, 'a', 'prev'), 'a')
  assert.equal(nextSelection(ids, 'c', 'prev'), 'b')
  assert.equal(nextSelection(ids, null, 'next'), 'a')
  assert.equal(nextSelection(ids, null, 'prev'), 'c')
  assert.equal(nextSelection(ids, 'gone', 'next'), 'a')
  assert.equal(nextSelection([], 'a', 'next'), null)
})

test('weekNeighbour crosses empty columns and clamps the row', () => {
  const cols = [['a', 'b', 'c'], [], ['d'], ['e', 'f']]
  assert.equal(weekNeighbour(cols, 'c', 'right'), 'd')
  assert.equal(weekNeighbour(cols, 'd', 'left'), 'a')
  assert.equal(weekNeighbour(cols, 'f', 'left'), 'd')
  assert.equal(weekNeighbour(cols, 'd', 'right'), 'e')
  assert.equal(weekNeighbour(cols, 'f', 'right'), 'f')
  assert.equal(weekNeighbour(cols, 'a', 'left'), 'a')
  assert.equal(weekNeighbour(cols, null, 'right'), 'a')
  assert.equal(weekNeighbour([[], []], null, 'right'), null)
})

test('moveDueDate changes the day and keeps the time', () => {
  assert.deepEqual(moveDueDate({ due_date: '2026-11-16', due_time: '17:00' }, '2026-11-18'), { due_date: '2026-11-18', due_time: '17:00' })
  assert.deepEqual(moveDueDate({ due_date: '2026-11-16', due_time: null }, '2026-11-18'), { due_date: '2026-11-18', due_time: null })
  assert.equal(moveDueDate({ due_date: '2026-11-18', due_time: '17:00' }, '2026-11-18'), null)
})
