#!/usr/bin/env node
/**
 * Compile every i18n message and fail on any that vue-i18n cannot parse.
 *
 * Why this exists: vue-i18n compiles a message LAZILY, the first time that
 * message is actually rendered. A message with invalid syntax therefore throws
 * nothing at build time and nothing at startup — it throws when the user reaches
 * the one screen that uses it, surfacing as a bare `SyntaxError: <code>` with a
 * minified stack that points at the compiler rather than at the message.
 *
 * The trap that motivated this: a literal `@` in a message. vue-i18n reads `@`
 * as the start of a linked message (`@:key` / `@.lower:key`), so plain prose
 * like "(mention files with @)" fails with INVALID_LINKED_FORMAT. A literal `@`
 * must be written `{'@'}`.
 *
 * Note that `baseCompile` reports problems through an `onError` callback and
 * does NOT throw, so a naive try/catch around it silently passes everything.
 *
 * It also checks the vi messages against the glossary (i18n-glossary.json):
 * terms such as Session and Project stay in English, so "phiên" or "dự án"
 * is reported with the term to use instead.
 *
 * Usage: pnpm check:i18n
 */
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { findBannedTerms } from './i18n-glossary.mjs'

// `@intlify/message-compiler` is vue-i18n's own dependency, not ours, and pnpm's
// strict layout hides it from this script. Resolve it the way vue-i18n would, so
// we always check against the exact compiler the app ships with — and so this
// needs no extra devDependency to drift out of sync.
const require = createRequire(import.meta.url)
const viaVueI18n = createRequire(require.resolve('vue-i18n'))
const { baseCompile, CompileErrorCodes } = viaVueI18n('@intlify/message-compiler')

/** Numeric code → name, so a failure reads as INVALID_LINKED_FORMAT not `10`. */
const CODE_NAMES = Object.fromEntries(
  Object.entries(CompileErrorCodes).map(([name, code]) => [code, name]),
)

// `URL.pathname` is `/D:/…` on Windows (and keeps %-escapes); fileURLToPath
// gives a real filesystem path on every OS.
const LOCALES_DIR = fileURLToPath(new URL('../src/i18n/locales/', import.meta.url))

const GLOSSARY = JSON.parse(readFileSync(new URL('./i18n-glossary.json', import.meta.url), 'utf8'))

// A banned translation fails the build, so the vi locale cannot drift from the
// glossary again.
const GLOSSARY_FAILS_BUILD = true

/** Walk a message tree, yielding `[dottedKey, message]` for every string leaf. */
function* leaves(node, prefix = '') {
  for (const [key, value] of Object.entries(node)) {
    const path = prefix ? `${prefix}.${key}` : key
    if (value && typeof value === 'object') yield* leaves(value, path)
    else if (typeof value === 'string') yield [path, value]
  }
}

const failures = []
const glossaryHits = []
let checked = 0

for (const locale of readdirSync(LOCALES_DIR)) {
  const dir = join(LOCALES_DIR, locale)
  for (const file of readdirSync(dir).filter((f) => f.endsWith('.json'))) {
    const namespace = file.replace(/\.json$/, '')
    const tree = JSON.parse(readFileSync(join(dir, file), 'utf8'))
    for (const [key, message] of leaves(tree)) {
      checked += 1
      const errors = []
      baseCompile(message, { onError: (e) => errors.push(e) })
      for (const e of errors) {
        failures.push({
          where: `${locale}/${namespace}.${key}`,
          code: `${CODE_NAMES[e.code] ?? 'UNKNOWN'} (${e.code})`,
          message,
        })
      }
      if (locale === 'vi') {
        for (const hit of findBannedTerms(message, GLOSSARY, `${namespace}.${key}`)) {
          glossaryHits.push({ where: `${locale}/${namespace}.${key}`, ...hit })
        }
      }
    }
  }
}

if (glossaryHits.length > 0) {
  const report = GLOSSARY_FAILS_BUILD ? console.error : console.warn
  const level = GLOSSARY_FAILS_BUILD ? 'error' : 'warning'
  report(`i18n glossary (${level}): ${glossaryHits.length} banned translations in vi — keep these terms in English (scripts/i18n-glossary.json)\n`)
  for (const h of glossaryHits) report(`  ${h.where}: "${h.word}" → ${h.term}`)
  const byTerm = {}
  for (const h of glossaryHits) byTerm[h.term] = (byTerm[h.term] ?? 0) + 1
  report(`\n  by term: ${Object.entries(byTerm).map(([term, n]) => `${term} ${n}`).join(', ')}\n`)
}
const glossaryFails = GLOSSARY_FAILS_BUILD && glossaryHits.length > 0

if (failures.length === 0) {
  console.log(`i18n: ${checked} messages compile cleanly`)
  process.exit(glossaryFails ? 1 : 0)
}

console.error(`i18n: ${failures.length} of ${checked} messages fail to compile\n`)
for (const f of failures) {
  console.error(`  ${f.where}`)
  console.error(`    ${f.code}`)
  console.error(`    ${JSON.stringify(f.message)}`)
  if (f.code.startsWith('INVALID_LINKED_FORMAT')) {
    console.error(`    hint: write a literal @ as {'@'}`)
  }
  console.error('')
}
process.exit(1)
