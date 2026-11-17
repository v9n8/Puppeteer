import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const tab = read('../../components/running/RunningSessionsTab.vue')
const view = read('../../views/RunningView.vue')
const limits = read('../../components/settings/LimitsSettings.vue')
const toastHost = read('../../components/ui/ToastHost.vue')

// Vue trims whitespace at the edge of an element, so a space inside the second
// span vanishes; it has to be an explicit text node between the spans.
test('Running tab: argument spans start without a lost leading space', () => {
  assert.doesNotMatch(tab, /<span class="font-mono text-muted-foreground"> \{\{/)
  assert.equal((tab.match(/\{\{ ' ' \}\}/g) ?? []).length, 3)
})

test('Limits: percent and "(stale)" are separated by an explicit space', () => {
  assert.match(limits, /\{\{ e\.percent \}\}%<template v-if="e\.stale">\{\{ ' ' \}\}\{\{ t\('settings\.sessions\.nowStale'\) \}\}/)
})

test('Running tab: filter chips use the shared 24px SegmentedControl (no per-screen height override)', () => {
  assert.doesNotMatch(tab, /<SegmentedControl[^>]*min-h-7/)
})

test('Running tab: row checkboxes sit in a 28px hit area', () => {
  assert.equal((tab.match(/<label class="-mx-1\.5 flex h-7 w-7 /g) ?? []).length, 2)
})

test('"Dừng tất cả" lives in the page header, not in the filter row', () => {
  assert.doesNotMatch(tab, /processes\.running\.action\.stopAllTitle/)
  assert.match(view, /#actions[\s\S]*processes\.running\.action\.stopAllTitle/)
})

test('Toast action link has a 28px hit area', () => {
  assert.match(toastHost, /v-if="t\.action"\s+type="button"\s+class="[^"]*min-h-7[^"]*text-primary/)
})
