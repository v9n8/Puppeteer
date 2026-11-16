// Pure helpers behind the file viewer's Markdown outline column and its
// "Compare with…" menu row.

export interface OutlineItem {
  level: number
  text: string
  slug: string
}

/** Heading text as a reader sees it: inline markdown (links, images, emphasis, code) removed. */
function plainHeading(raw: string): string {
  return raw
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[`*_~]/g, '')
    .replace(/<[^>]+>/g, '')
    .trim()
}

function slugOf(text: string): string {
  const base = text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, '')
    .trim()
    .replace(/\s+/g, '-')
  return base || 'section'
}

/**
 * The ATX headings (`# …` to `###### …`) of a Markdown source, in order.
 * Lines inside fenced code blocks are not headings. Slugs are unique within the
 * result (`intro`, `intro-1`, …), so each one can serve as an element id.
 */
export function markdownOutline(source: string): OutlineItem[] {
  const items: OutlineItem[] = []
  const used = new Map<string, number>()
  let fence: { char: string; len: number } | null = null
  for (const line of source.split(/\r?\n/)) {
    if (fence) {
      const close = /^ {0,3}(`{3,}|~{3,})\s*$/.exec(line)
      if (close && close[1][0] === fence.char && close[1].length >= fence.len) fence = null
      continue
    }
    const open = /^ {0,3}(`{3,}|~{3,})/.exec(line)
    if (open) {
      fence = { char: open[1][0], len: open[1].length }
      continue
    }
    const m = /^ {0,3}(#{1,6})[ \t]+(.*?)(?:[ \t]+#+)?[ \t]*$/.exec(line)
    if (!m) continue
    const text = plainHeading(m[2])
    if (!text) continue
    const base = slugOf(text)
    const n = used.get(base) ?? 0
    used.set(base, n + 1)
    items.push({ level: m[1].length, text, slug: n ? `${base}-${n}` : base })
  }
  return items
}

/** Whether the outline column has anything to list (headings of level 1–3). */
export function hasHeadings(source: string): boolean {
  return markdownOutline(source).some(i => i.level <= 3)
}

const IMAGE_EXT = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico', 'avif', 'apng']
// Files the viewer cannot show as text (media, PDF, office documents, archives, binaries):
// the same families as FileViewer's own classification.
const NOT_COMPARABLE_EXT = [
  'mp4', 'webm', 'mov', 'm4v', 'ogv', 'mkv',
  'mp3', 'wav', 'ogg', 'oga', 'flac', 'm4a', 'aac',
  'pdf',
  'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods', 'odp', 'rtf', 'pages', 'numbers', 'key',
  'zip', 'tar', 'gz', 'tgz', 'bz2', 'xz', 'rar', '7z',
  'exe', 'dmg', 'pkg', 'app', 'msi', 'deb', 'bin', 'so', 'dylib', 'dll', 'o', 'a', 'class', 'jar', 'wasm',
  'ttf', 'otf', 'woff', 'woff2', 'eot', 'psd', 'ai', 'sketch', 'fig', 'xcf', 'heic', 'tiff', 'tif', 'raw',
]

/** How a file can be compared: side by side as images, as a text diff, or not at all. */
export function compareKind(path: string): 'image' | 'text' | null {
  const base = (path.split('/').pop() ?? path).toLowerCase()
  const ext = base.includes('.') ? base.slice(base.lastIndexOf('.') + 1) : ''
  if (IMAGE_EXT.includes(ext)) return 'image'
  if (NOT_COMPARABLE_EXT.includes(ext)) return null
  return 'text'
}
