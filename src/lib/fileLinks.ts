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
