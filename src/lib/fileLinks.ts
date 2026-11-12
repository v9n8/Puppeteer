// Shared helpers for turning text references (inline `<code>` filenames, relative
// markdown links) into clickable links to files in a project. Used by RunView's
// own markdown containers and by the reusable FileViewer component.

// Trailing punctuation a path picks up from the sentence around it.
const TRAILING_PUNCT = /[)\].,:;'"`]+$/

/** A path with the line it points at, when the reference named one. */
export interface FileRef {
  path: string
  line: number | null
}

/**
 * Split a trailing line reference off a path token: `a.ts:12`, `a.ts:12:5`
 * (line:column), `a.ts:12-20`, `a.ts#L12`, `a.ts#L12-L20`, `a.ts#12`.
 */
export function splitLineRef(raw: string): FileRef {
  const m = raw.match(/^(.+?)(?:#L?(\d+)(?:-L?\d+)?|:(\d+)(?::\d+)?(?:-\d+)?)$/i)
  if (!m) return { path: raw, line: null }
  return { path: m[1], line: parseInt(m[2] ?? m[3], 10) }
}

/**
 * Resolve a raw token (an inline-code filename or a relative link href) to a
 * concrete project-relative path, or null when it doesn't unambiguously match a
 * known project file. `fileSet` is the set of project-relative file paths.
 * A line reference (`:12`, `#L12`) is ignored here; `parseLineRef` reads it.
 * An absolute path under `root` (the project folder) matches as relative.
 */
export function matchProjectFile(raw: string, fileSet: Set<string>, root?: string | null): string | null {
  if (!raw || raw.length > 200 || /\s/.test(raw)) return null
  let token = splitLineRef(raw.trim().replace(TRAILING_PUNCT, '')).path
  if (token.startsWith('./')) token = token.slice(2)
  const rootPrefix = root ? root.replace(/\/+$/, '') + '/' : null
  if (rootPrefix && token.startsWith(rootPrefix)) token = token.slice(rootPrefix.length)
  if (!token) return null
  if (fileSet.has(token)) return token
  // Basename / suffix match: unambiguous tail like `dir/file.ts`.
  if (token.includes('/')) {
    for (const p of fileSet) if (p === token || p.endsWith('/' + token)) return p
  } else if (/\.[a-zA-Z0-9]+$/.test(token)) {
    // Bare filename with an extension — match only if exactly one file ends with it.
    let hit: string | null = null
    for (const p of fileSet) {
      if (p === token || p.endsWith('/' + token)) {
        if (hit) return null // ambiguous
        hit = p
      }
    }
    if (hit) return hit
  }
  return null
}

/** Extract a target line number from a `#L42` / `#42` / `:42` / `:42:7` / `:42-50` reference. */
export function parseLineRef(ref: string): number | null {
  return splitLineRef(ref.trim().replace(TRAILING_PUNCT, '')).line
}

/**
 * The file a markdown link points at: the href decoded, its `#fragment` dropped
 * and its line reference read off. Null for an in-page anchor or a URL with a
 * scheme (`https:`, `vscode:`, …); a one-letter scheme is a Windows drive.
 */
export function hrefFileRef(href: string): FileRef | null {
  if (/^[a-z][a-z0-9+.-]+:/i.test(href)) return null
  let decoded = href
  try {
    decoded = decodeURI(href)
  } catch { /* malformed escape: keep the href as written */ }
  const path = splitLineRef(decoded.replace(/#.*$/, '')).path
  return path ? { path, line: parseLineRef(decoded) } : null
}

// ── Bare paths in prose (UX-44) ─────────────────────────────────────────────

// What a bare path in prose must end with. A list, not "any extension", so
// `array.map`, `1.2.3`, `e.g.` and `Node.js`-style prose stay text; `map` is left
// out on purpose (source maps are rarely what a sentence names).
const PATH_EXTENSIONS = new Set([
  'ts', 'tsx', 'mts', 'cts', 'js', 'jsx', 'mjs', 'cjs', 'vue', 'svelte', 'astro',
  'json', 'jsonc', 'json5', 'md', 'mdx', 'markdown', 'txt', 'rst', 'adoc',
  'rs', 'go', 'py', 'pyi', 'rb', 'php', 'java', 'kt', 'kts', 'swift', 'm', 'mm',
  'c', 'h', 'cc', 'cpp', 'cxx', 'hpp', 'hh', 'cs', 'fs', 'scala', 'dart', 'lua',
  'ex', 'exs', 'erl', 'hs', 'clj', 'pl', 'r', 'jl', 'zig', 'nim',
  'sh', 'bash', 'zsh', 'fish', 'ps1', 'bat', 'cmd',
  'sql', 'graphql', 'gql', 'proto', 'prisma',
  'yml', 'yaml', 'toml', 'ini', 'cfg', 'conf', 'env', 'properties', 'plist', 'xml', 'lock',
  'html', 'htm', 'css', 'scss', 'sass', 'less', 'svg',
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'ico', 'pdf', 'csv', 'tsv', 'log', 'diff', 'patch',
  'tf', 'hcl', 'gradle', 'cmake', 'mk', 'ipynb', 'wasm',
])

// A URL is matched first only so its path part is skipped. A path: optional
// `/`, `./` or `../` lead, `dir/` segments (a leading dot allowed: `.github/`),
// a file name with an extension, then an optional line reference
// (`:12`, `:12:5`, `:12-20`, `#L12`, `#L12-L20`).
const PLAIN_PATH_RE =
  /([a-z][a-z0-9+.-]*:\/\/[^\s<>"'`]+)|((?:\/|(?:\.{1,2}\/)+)?(?:\.?[\w@+-][\w.@+-]*\/)*\.?[\w@+-][\w.@+-]*\.([a-z0-9]+)(?::\d+(?::\d+)?(?:-\d+)?|#L\d+(?:-L?\d+)?)?)/gi

// A path glued to these is part of something else: `~/x.ts` (no home
// expansion), `C:/x.ts`, `host:/x.ts`, the tail of a longer token.
const PATH_GLUE_BEFORE = /[\w.@/:~\\-]/
const PATH_GLUE_AFTER = /[\w/@+\\-]/

/**
 * The path an inline-code token names when the project list doesn't know it
 * (git-ignored, outside the project, created since the list loaded), for the
 * backend to confirm; null when the token doesn't read as a file path. Ignore
 * rules only keep files out of git, so they must not keep a link from opening.
 */
export function codePathCandidate(raw: string): string | null {
  if (!raw || raw.length > 200 || /\s/.test(raw)) return null
  const path = splitLineRef(raw.replace(TRAILING_PUNCT, '')).path
  const ext = path.match(/\.([a-z0-9]+)$/i)?.[1]
  return ext && PATH_EXTENSIONS.has(ext.toLowerCase()) ? path : null
}

/** A bare path found in prose: where it sits, the text as written, the path without its line. */
export interface PlainPath {
  start: number
  end: number
  text: string
  path: string
  line: number | null
}

/** Find the bare file paths in a run of prose (candidates; whether they exist is asked later). */
export function findPlainPaths(text: string): PlainPath[] {
  const out: PlainPath[] = []
  PLAIN_PATH_RE.lastIndex = 0
  let m: RegExpExecArray | null
  while ((m = PLAIN_PATH_RE.exec(text))) {
    const raw = m[2]
    if (raw === undefined) continue // a URL
    const start = m.index
    const end = start + raw.length
    if (!PATH_EXTENSIONS.has(m[3].toLowerCase())) continue
    if (start > 0 && PATH_GLUE_BEFORE.test(text[start - 1])) continue
    if (end < text.length && PATH_GLUE_AFTER.test(text[end])) continue
    const ref = splitLineRef(raw)
    out.push({ start, end, text: raw, path: ref.path, line: ref.line })
  }
  return out
}

/**
 * Whether paths name a real file, asked of the backend in batches and
 * remembered. Every `check` made in the same tick goes out as one call (at most
 * `max` paths each), so opening a long session costs a few calls, not one per
 * message. `clear` forgets every answer, and drops the ones still in flight.
 */
export interface PathChecker {
  /** The remembered answer for `path`, or undefined when it was never asked. */
  known(path: string): boolean | undefined
  /** Ask about the paths not answered yet; resolves once they are. */
  check(paths: string[]): Promise<void>
  clear(): void
}

export function createPathChecker(stat: (paths: string[]) => Promise<boolean[]>, max = 500): PathChecker {
  const cache = new Map<string, boolean>()
  const pending = new Map<string, Promise<void>>()
  let batch: string[] = []
  let batchDone: Promise<void> | null = null
  let gen = 0

  async function send(paths: string[], done: Promise<void>) {
    const at = gen
    try {
      for (let i = 0; i < paths.length; i += max) {
        const chunk = paths.slice(i, i + max)
        const hits = await stat(chunk)
        if (at === gen) chunk.forEach((p, j) => cache.set(p, !!hits[j]))
      }
    } finally {
      for (const p of paths) if (pending.get(p) === done) pending.delete(p)
    }
  }

  function check(paths: string[]): Promise<void> {
    const waits: Promise<void>[] = []
    for (const p of new Set(paths)) {
      if (cache.has(p)) continue
      let wait = pending.get(p)
      if (!wait) {
        if (!batchDone) {
          const done: Promise<void> = Promise.resolve().then(() => {
            const queued = batch
            batch = []
            batchDone = null
            return send(queued, done)
          })
          batchDone = done
        }
        batch.push(p)
        wait = batchDone
        pending.set(p, wait)
      }
      waits.push(wait)
    }
    return Promise.all(waits).then(() => undefined)
  }

  return {
    known: (path) => cache.get(path),
    check,
    clear() {
      gen++
      cache.clear()
      pending.clear()
    },
  }
}

// Where a bare path is not linked: already a link or a button, code (inline
// code has its own matcher), or a rendered diagram.
const NO_PATH_LINKS = 'a, code, pre, button, textarea, input, svg, .file-link, .mermaid, .katex'

// The latest pass per element: a slow answer for an older render must not
// rewrite the newer one.
const linkPasses = new WeakMap<HTMLElement, number>()

/**
 * Turn the bare paths in `el`'s rendered text into `span.file-link[data-file-path]`
 * when they name a real file: a project file `matcher` knows at once, else one
 * `checker` confirms (outside the project, ignored, created since the list
 * loaded). Only for markup Vue doesn't patch node by node (`v-html`).
 */
export function linkPlainPaths(
  el: HTMLElement,
  matcher: ((raw: string) => string | null) | undefined,
  checker: PathChecker | undefined,
) {
  if (!matcher && !checker) return
  const pass = (linkPasses.get(el) ?? 0) + 1
  linkPasses.set(el, pass)
  const found: Array<{ node: Text; data: string; paths: PlainPath[] }> = []
  const walker = el.ownerDocument.createTreeWalker(el, NodeFilter.SHOW_TEXT)
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const node = n as Text
    const skip = node.parentElement?.closest(NO_PATH_LINKS)
    if (skip && el.contains(skip)) continue
    const paths = findPlainPaths(node.data)
    if (paths.length) found.push({ node, data: node.data, paths })
  }
  if (!found.length) return

  const resolve = (p: PlainPath) => matcher?.(p.text) ?? (checker?.known(p.path) ? p.path : null)
  const apply = () => {
    if (linkPasses.get(el) !== pass) return
    for (const { node, data, paths } of found) {
      if (node.data !== data || !el.contains(node)) continue
      const frag = el.ownerDocument.createDocumentFragment()
      let last = 0
      for (const p of paths) {
        const path = resolve(p)
        if (!path) continue
        const link = el.ownerDocument.createElement('span')
        link.className = 'file-link'
        link.setAttribute('data-file-path', path)
        link.textContent = p.text
        frag.append(data.slice(last, p.start), link)
        last = p.end
      }
      if (!last) continue
      frag.append(data.slice(last))
      node.replaceWith(frag)
    }
  }

  const unknown = found.flatMap((f) => f.paths)
    .filter((p) => !matcher?.(p.text) && checker?.known(p.path) === undefined)
  if (!checker || !unknown.length) return apply()
  checker.check(unknown.map((p) => p.path)).then(apply, () => { /* backend unavailable: leave as text */ })
}

/**
 * Mark inline `<code>` elements that name a project file as clickable
 * `code.file-link[data-file-path]`. `matcher` resolves a token to a path.
 */
export function decorateFileLinks(el: HTMLElement, matcher: (raw: string) => string | null) {
  el.querySelectorAll('code').forEach((code) => {
    if (code.closest('pre')) return
    const path = matcher((code.textContent ?? '').trim())
    if (path) {
      code.classList.add('file-link')
      code.setAttribute('data-file-path', path)
    } else {
      code.classList.remove('file-link')
      code.removeAttribute('data-file-path')
    }
  })
}
