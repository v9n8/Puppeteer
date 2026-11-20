// RT-20 (#5): the Add Project modal can re-pick its folder; each parent re-runs its own picker.
// Run: node --test src/lib/__tests__/addProjectRepick.test.mjs
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

test('the modal has a "Choose folder" button beside the path that emits change-folder', () => {
  const src = read('../../components/projects/AddProjectModal.vue')
  assert.match(src, /'change-folder': \[\]/)
  assert.match(src, /@click="emit\('change-folder'\)"[\s\S]*?t\('projects\.chooseFolder'\)/)
})

test('both parents re-open their picker on change-folder', () => {
  assert.match(read('../../views/ProjectsView.vue'), /<AddProjectModal [^>]*@change-folder="handleAdd"/)
  const switcher = read('../../components/ProjectSwitcher.vue')
  assert.match(switcher, /<AddProjectModal [^>]*@change-folder="startAdd"/)
  // Re-picking a folder that is already a Project opens it and closes the modal.
  const start = switcher.indexOf('async function startAdd(')
  const body = switcher.slice(start, switcher.indexOf('\n}\n', start))
  assert.match(body, /if \(existing\) \{[\s\S]*?pending\.value = null[\s\S]*?choose\(existing\.id\)/)
})
