// The HTML preview iframe in components/FileViewer.vue renders a file from disk
// over asset://, whose scope is every path. With `allow-same-origin` the page
// shares the asset origin, so its scripts could fetch() any file on the machine
// (~/.ssh/id_rsa) and post it out. Pinned so the flag cannot come back quietly.
// Run: node --test src/lib/__tests__/htmlPreviewSandbox.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const viewer = readFileSync(new URL('../../components/FileViewer.vue', import.meta.url), 'utf8')

function iframeSandboxes() {
  const iframes = viewer.match(/<iframe\b[^>]*>/g) ?? []
  assert.ok(iframes.length > 0, 'iframe not found in FileViewer.vue')
  return iframes.map((tag) => {
    const m = tag.match(/\ssandbox="([^"]*)"/)
    assert.ok(m, `iframe without a static sandbox attribute: ${tag}`)
    return m[1].split(/\s+/).filter(Boolean)
  })
}

test('the HTML preview iframe has an opaque origin (no allow-same-origin)', () => {
  for (const flags of iframeSandboxes()) {
    assert.ok(!flags.includes('allow-same-origin'), flags.join(' '))
  }
})

test('the HTML preview still runs its own scripts', () => {
  for (const flags of iframeSandboxes()) {
    assert.ok(flags.includes('allow-scripts'), flags.join(' '))
  }
})
