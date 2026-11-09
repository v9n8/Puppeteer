// Terraform config scanner for the Puppeteer terraform shim.
//
// Before a terraform call is sent to the broker, the shim scans the config for
// code that runs OUTSIDE Terraform — `data "external"`, `provisioner
// "local-exec"`, `data "http"` — because during `plan`/`validate`/`console` it
// would execute with the linked cloud credentials. The broker turns an
// otherwise-allowed evaluating command into an approval prompt when found.
//
// Scope: `*.tf` / `*.tf.json` in the working dir, recursively in LOCAL module
// sources (`module "x" { source = "./..." }` / `"../..."`, also `.\` / `..\`
// like Terraform), and everything under `.terraform/modules` (downloaded
// remote modules).
// Fail-closed: any read error, hitting a scan limit, or a file whose module
// blocks cannot be parsed with certainty counts as detected.

import fs from 'node:fs'
import path from 'node:path'

const MAX_DEPTH = 12
const MAX_FILES = 2000
const MAX_HITS = 5

const HCL_PATTERNS = [
  [/\bdata\s+"external"/, 'data.external'],
  [/"local-exec"/, 'local-exec'],
  [/\bdata\s+"http"/, 'data.http'],
]

// .tf.json: a simple substring heuristic is enough (keys under "data" /
// "provisioner").
function jsonKinds(text) {
  const kinds = []
  const hasData = /"data"\s*:/.test(text)
  if (hasData && /"external"\s*:/.test(text)) kinds.push('data.external')
  if (/"local-exec"/.test(text)) kinds.push('local-exec')
  if (hasData && /"http"\s*:/.test(text)) kinds.push('data.http')
  return kinds
}

function hclKinds(text) {
  return HCL_PATTERNS.filter(([re]) => re.test(text)).map(([, kind]) => kind)
}

// Local module sources. Only the `source` argument of a top-level `module`
// block is a module path: `provisioner "file" { source = "./run.sh" }` is a
// file, and following it as a dir would report it unreadable. So module
// blocks are parsed (HCL native syntax: hashicorp/hcl hclsyntax/spec.md; JSON:
// hashicorp/hcl json/spec.md). Terraform also accepts `.\` / `..\` and turns
// every `\` into `/` on every OS (hashicorp/terraform
// internal/getmodules/moduleaddrs/source_parsing.go, moduleSourceLocalPrefixes
// + parseModuleSourceLocal), so match the decoded string value, not the raw
// text: `".\\mod"` in a .tf file is `.\mod`.
const LOCAL_SOURCE_PREFIXES = ['./', '../', '.\\', '..\\']
const IDENT = /[\p{L}_][\p{L}\p{N}_-]*/uy
// A module source must be a literal; `${` / `%{` (even escaped) means it is not.
const TEMPLATE = /[$%]\{/
const HCL_ESCAPES = { n: '\n', r: '\r', t: '\t', '"': '"', '\\': '\\' }

// HCL quoted-string escapes (\n \r \t \" \\ \uNNNN \UNNNNNNNN). Template
// sequences are left as-is: a module source must be a literal anyway.
function decodeHcl(body) {
  return body.replace(/\\(?:u([0-9a-fA-F]{4})|U([0-9a-fA-F]{8})|(.))/gs, (m, u, U, c) => {
    if (u || U) {
      const cp = parseInt(u || U, 16)
      return cp <= 0x10ffff ? String.fromCodePoint(cp) : m
    }
    return HCL_ESCAPES[c] ?? m
  })
}

function decodeJson(body) {
  try {
    return JSON.parse(`"${body}"`)
  } catch {
    return body
  }
}

// `source` values of top-level `module` blocks in HCL native syntax, decoded.
// Returns null when unsure (unbalanced brackets, unterminated string /
// heredoc / comment, a source that is not a plain literal): the caller then
// reports the file and follows every `source` string, as before.
function hclModuleSources(text) {
  const out = []
  // Frames: brackets `{ [ (` (code), `str` / `heredoc` (template), and `${` /
  // `%{` inside a template (code closed by `}`).
  const stack = []
  let stmt = [] // tokens of the current top-level / module-body line
  let i = text.charCodeAt(0) === 0xfeff ? 1 : 0
  const isIdent = (tok, name) => tok?.t === 'ident' && tok.v === name
  const atBody = () => stack.length === 0 || (stack.length === 1 && stack[0].module)
  const endStmt = () => {
    if (stack.length === 1 && stack[0].module && isIdent(stmt[0], 'source')) {
      const [, eq, val, extra] = stmt
      if (eq?.t !== 'eq' || val?.t !== 'str' || extra || TEMPLATE.test(val.body)) return false
      out.push(decodeHcl(val.body))
    }
    stmt = []
    return true
  }
  const token = (t, v) => {
    if (atBody()) stmt.push({ t, v })
  }
  while (i < text.length) {
    const top = stack[stack.length - 1]
    const c = text[i]
    if (top?.kind === 'str' || top?.kind === 'heredoc') {
      if (top.kind === 'heredoc' && text[i - 1] === '\n') {
        top.end.lastIndex = i
        if (top.end.exec(text)) {
          stack.pop()
          i = top.end.lastIndex
          continue
        }
      }
      const two = text.slice(i, i + 2)
      if (two === '${' || two === '%{') {
        stack.push({ kind: '}' })
        i += 2
      } else if (two === '$$' || two === '%%') {
        i += 2 // `$${` / `%%{` stay literal; the `{` is skipped as text
      } else if (top.kind === 'heredoc') {
        i++
      } else if (c === '"') {
        top.tok.body = text.slice(top.tok.start, i)
        stack.pop()
        i++
      } else if (c === '\n' || (c === '\\' && text[i + 1] === '\n')) {
        return null // quoted templates cannot hold a newline
      } else {
        i += c === '\\' ? 2 : 1
      }
      continue
    }
    if (c === ' ' || c === '\t' || c === '\r') {
      i++
    } else if (c === '\n') {
      if (atBody() && !endStmt()) return null
      i++
    } else if (c === '#' || text.startsWith('//', i)) {
      const nl = text.indexOf('\n', i)
      i = nl < 0 ? text.length : nl
    } else if (text.startsWith('/*', i)) {
      const end = text.indexOf('*/', i + 2)
      if (end < 0) return null
      i = end + 2
    } else if (c === '"') {
      const tok = { t: 'str', start: i + 1 }
      if (atBody()) stmt.push(tok)
      stack.push({ kind: 'str', tok })
      i++
    } else if (text.startsWith('<<', i)) {
      const m = new RegExp(`<<-?(${IDENT.source})\\r?\\n`, 'uy')
      m.lastIndex = i
      const h = m.exec(text)
      if (!h) return null
      token('heredoc')
      // Closing marker: the identifier alone on its line, spaces before it.
      stack.push({ kind: 'heredoc', end: new RegExp(`[ \\t]*${h[1]}(?=\\r?\\n|$)`, 'uy') })
      i = m.lastIndex
    } else if (c === '{' || c === '[' || c === '(') {
      const module = stack.length === 0 && c === '{' && isIdent(stmt[0], 'module')
      if (stack.length === 0) stmt = []
      else token('other')
      stack.push({ kind: { '{': '}', '[': ']', '(': ')' }[c], module })
      i++
    } else if (c === '}' || c === ']' || c === ')') {
      if (top?.kind !== c) return null
      if (top.module && !endStmt()) return null
      stack.pop()
      if (stack.length === 0) stmt = []
      i++
    } else {
      IDENT.lastIndex = i
      const id = IDENT.exec(text)
      if (id) {
        token('ident', id[0])
        i = IDENT.lastIndex
      } else {
        // `==`, `!=`, `<=`, `>=`, `=>` must not read as `=`.
        const op = /[=!<>]=|=>/y
        op.lastIndex = i
        const len = op.test(text) ? 2 : 1
        token(len === 1 && c === '=' ? 'eq' : 'other')
        i += len
      }
    }
  }
  if (stack.length || !endStmt()) return null
  return out
}

// Minimal JSON parser keeping duplicate keys (objects are `{ entries }`):
// HCL JSON bodies allow the same property more than once (json/spec.md), and
// JSON.parse would keep only the last `"module"`.
function parseJsonEntries(text) {
  let i = text.charCodeAt(0) === 0xfeff ? 1 : 0
  const STR = /"(?:[^"\\\u0000-\u001f]|\\.)*"/y
  const PRIM = /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null/y
  const ws = () => {
    while (i < text.length && ' \t\n\r'.includes(text[i])) i++
  }
  const take = (re) => {
    re.lastIndex = i
    const m = re.exec(text)
    if (!m) throw new Error('json')
    i = re.lastIndex
    return m[0]
  }
  const value = () => {
    ws()
    if (text[i] === '"') return JSON.parse(take(STR))
    if (text[i] !== '{' && text[i] !== '[') return take(PRIM) && null
    const isObj = text[i++] === '{'
    const close = isObj ? '}' : ']'
    const items = []
    ws()
    if (text[i] === close) {
      i++
    } else {
      for (;;) {
        if (isObj) {
          ws()
          const k = JSON.parse(take(STR))
          ws()
          if (text[i++] !== ':') throw new Error('json')
          items.push([k, value()])
        } else {
          items.push(value())
        }
        ws()
        const c = text[i++]
        if (c === close) break
        if (c !== ',') throw new Error('json')
      }
    }
    return isObj ? { entries: items } : items
  }
  const v = value()
  ws()
  if (i < text.length) throw new Error('json')
  return v
}

// `source` values of `"module"` blocks in .tf.json: `{"module": {"<name>":
// {body}}}`, where each level may also be an array of objects. Null when unsure.
function jsonModuleSources(text) {
  let root
  try {
    root = parseJsonEntries(text)
  } catch {
    return null
  }
  // Strings are `"//"` comments (or invalid config Terraform rejects).
  const objects = (v) => (Array.isArray(v) ? v : [v]).filter((o) => typeof o !== 'string')
  const isObj = (o) => o !== null && typeof o === 'object' && Array.isArray(o.entries)
  if (!isObj(root)) return null
  const out = []
  for (const [type, labels] of root.entries) {
    if (type !== 'module') continue
    for (const named of objects(labels)) {
      if (!isObj(named)) return null
      for (const [, bodies] of named.entries) {
        for (const body of objects(bodies)) {
          if (!isObj(body)) return null
          for (const [k, v] of body.entries) {
            if (k !== 'source') continue
            if (typeof v !== 'string' || TEMPLATE.test(v)) return null
            out.push(v)
          }
        }
      }
    }
  }
  return out
}

// Every `source = "..."` string anywhere in the file (the pre-parse
// behaviour), used when the module blocks cannot be parsed with certainty.
function anySources(text, isJson) {
  const re = isJson
    ? /"source"\s*:\s*"((?:[^"\\]|\\.)*)"/g
    : /\bsource\s*=\s*"((?:[^"\\]|\\.)*)"/g
  return [...text.matchAll(re)].map((m) => (isJson ? decodeJson(m[1]) : decodeHcl(m[1])))
}

// `{ sources, certain }`: local module dirs to follow (`\` → `/`).
function localSources(text, isJson) {
  const mods = isJson ? jsonModuleSources(text) : hclModuleSources(text)
  const sources = (mods ?? anySources(text, isJson))
    .filter((src) => LOCAL_SOURCE_PREFIXES.some((p) => src.startsWith(p)))
    .map((src) => src.replaceAll('\\', '/'))
  return { sources, certain: mods !== null }
}

/**
 * Resolve terraform's working dir: the `-chdir=<dir>` global option (only
 * valid before the subcommand), else `cwd`.
 * @param {string[]} argv
 * @param {string} cwd
 * @returns {string}
 */
export function terraformWorkDir(argv, cwd) {
  for (const a of argv) {
    if (!a.startsWith('-')) break // first positional = subcommand
    const m = /^--?chdir=(.*)$/.exec(a)
    if (m) return path.resolve(cwd, m[1])
  }
  return path.resolve(cwd)
}

/**
 * Scan a terraform working dir for code that runs outside Terraform.
 * @param {string} workDir
 * @returns {{detected: boolean, hits: string[]}} hits are `relpath:kind`
 */
export function scanTerraformExternalCode(workDir) {
  const root = path.resolve(workDir)
  const hits = []
  let detected = false
  let files = 0
  const visited = new Set()
  const label = (p) => path.relative(root, p) || path.basename(p)
  const hit = (p, kind) => {
    detected = true
    if (hits.length < MAX_HITS) hits.push(`${label(p)}:${kind}`)
  }

  // `recurseAll`: walk every subdir (used for .terraform/modules).
  const scanDir = (dir, depth, recurseAll) => {
    if (hits.length >= MAX_HITS) return
    if (depth > MAX_DEPTH) return hit(dir, 'scan-depth-limit')
    let real
    try {
      real = fs.realpathSync(dir)
    } catch {
      return hit(dir, 'unreadable')
    }
    if (visited.has(real)) return // cycle / shared module
    visited.add(real)
    let entries
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true })
    } catch {
      return hit(dir, 'unreadable')
    }
    for (const e of entries) {
      if (hits.length >= MAX_HITS) return
      const p = path.join(dir, e.name)
      if (e.isDirectory()) {
        if (recurseAll) scanDir(p, depth + 1, true)
        continue
      }
      const isJson = e.name.endsWith('.tf.json')
      if (!isJson && !e.name.endsWith('.tf')) continue
      if (++files > MAX_FILES) return hit(dir, 'scan-file-limit')
      let text
      try {
        text = fs.readFileSync(p, 'utf8')
      } catch {
        hit(p, 'unreadable')
        continue
      }
      for (const kind of isJson ? jsonKinds(text) : hclKinds(text)) hit(p, kind)
      const { sources, certain } = localSources(text, isJson)
      if (!certain) hit(p, 'unparsed')
      for (const src of sources) {
        scanDir(path.resolve(path.dirname(p), src), depth + 1, false)
      }
    }
  }

  scanDir(root, 0, false)
  const downloaded = path.join(root, '.terraform', 'modules')
  if (fs.existsSync(downloaded)) scanDir(downloaded, 0, true)
  return { detected, hits }
}
