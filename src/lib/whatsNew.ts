// "What's new" (`app.whats_new`): the release notes bundled with the app and the
// pure decision of what to show at launch (tested in __tests__/whatsNew.test.mjs).
// A launch after an upgrade shows one toast (components/WhatsNewToast.vue); the
// notes themselves are the "What's new" card of Settings › About. Notes ship with
// the build, never fetched, so this works offline and opens no network path.

/** Settings row holding the newest version whose notes were offered. Not in a backup. */
export const SEEN_SETTING_KEY = 'whats_new_seen_version'

export type ChangeKind = 'added' | 'improved' | 'changed' | 'fixed'

export interface ReleaseNote {
  kind: ChangeKind
  en: string
  vi: string
}

export interface Release {
  /**
   * Same string as `version` in tauri.conf.json (no leading `v`). Empty while the
   * release is not cut: the card shows it as the next release and the launch
   * toast never does. Fill it in the commit that bumps the version.
   */
  version: string
  /** `YYYY-MM-DD`. */
  date: string
  items: ReleaseNote[]
}

/**
 * Notes per release, newest first. Add the entry in the commit that bumps the
 * version: a release without one shows no toast. Older releases are not written
 * back (QĐ-11).
 */
export const RELEASES: Release[] = [
  {
    version: '',
    date: '2026-11-16',
    items: [
      {
        kind: 'changed',
        en: 'Settings is a screen with a page for each section, in four groups: General, AI, Connections and System.',
        vi: 'Cài đặt là một màn với trang riêng cho từng mục, chia bốn nhóm: Chung, AI, Kết nối và Hệ thống.',
      },
      {
        kind: 'changed',
        en: 'The sidebar has three groups (Work, My work, Agent), with Stats, Trash and Settings at the bottom.',
        vi: 'Thanh bên có ba nhóm (Làm việc, Việc của tôi, Agent), với Thống kê, Thùng rác và Cài đặt ở chân.',
      },
      {
        kind: 'added',
        en: 'A command box in the title bar, and Cmd/Ctrl+Shift+F to search across all sessions.',
        vi: 'Ô lệnh trên thanh tiêu đề, và Cmd/Ctrl+Shift+F để tìm trong mọi Session.',
      },
      {
        kind: 'added',
        en: 'Settings › Flag manages the flags your sessions share in one place.',
        vi: 'Cài đặt › Flag quản lý các flag dùng chung của Session ở một chỗ.',
      },
      {
        kind: 'improved',
        en: 'Files open as tabs of their Session instead of new windows.',
        vi: 'Tệp mở thành tab trong Session thay vì cửa sổ mới.',
      },
      {
        kind: 'added',
        en: 'Settings › About shows the version, checks for updates on demand and lists what is new. Click the version in the sidebar to open it.',
        vi: 'Cài đặt › Giới thiệu hiện số version, cho kiểm tra cập nhật khi cần và liệt kê điều mới. Bấm số version ở thanh bên để mở.',
      },
    ],
  },
]

/** How many notes of each kind a release has, in the order of the card's summary. */
export function kindCounts(release: Release): { kind: ChangeKind; count: number }[] {
  const order: ChangeKind[] = ['added', 'improved', 'changed', 'fixed']
  return order
    .map((kind) => ({ kind, count: release.items.filter((i) => i.kind === kind).length }))
    .filter((c) => c.count > 0)
}

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/

type Parsed = { core: number[]; pre: string[] }

function parse(version: string): Parsed | null {
  const m = SEMVER.exec(version.trim())
  if (!m) return null
  return { core: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] ? m[4].split('.') : [] }
}

function comparePre(a: string[], b: string[]): number {
  // A release ranks above its pre-releases.
  if (!a.length || !b.length) return Math.sign(b.length - a.length)
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    const an = /^\d+$/.test(a[i])
    const bn = /^\d+$/.test(b[i])
    if (an && bn) {
      const d = Number(a[i]) - Number(b[i])
      if (d) return Math.sign(d)
    } else if (an !== bn) {
      return an ? -1 : 1
    } else if (a[i] !== b[i]) {
      return a[i] < b[i] ? -1 : 1
    }
  }
  return Math.sign(a.length - b.length)
}

/**
 * Semver precedence like `updates::is_newer` on the Rust side: -1, 0 or 1, or
 * null when either side is not a version. Build metadata (`+…`) never counts.
 */
export function compareVersions(a: string, b: string): number | null {
  const pa = parse(a)
  const pb = parse(b)
  if (!pa || !pb) return null
  for (let i = 0; i < 3; i++) {
    const d = pa.core[i] - pb.core[i]
    if (d) return Math.sign(d)
  }
  return comparePre(pa.pre, pb.pre)
}

/**
 * What to do at launch, given the running version and the stored one:
 * - `notes`: releases after `seen` up to `current`, newest first (empty: no popup);
 * - `markSeen`: the version to store, or null to leave the row as it is.
 *
 * No stored version (first launch with this feature) records `current` without a
 * popup: a fresh install cannot be told from an upgrade here, and a fresh install
 * should open on the app, not on release notes. The stored version only moves
 * up, so going back to an older build and up again never shows a release twice.
 */
export function whatsNewAtLaunch(
  current: string,
  seen: string | null | undefined,
  releases: Release[],
): { notes: Release[]; markSeen: string | null } {
  if (!parse(current)) return { notes: [], markSeen: null }
  if (!seen || !parse(seen)) return { notes: [], markSeen: current }
  if (compareVersions(current, seen)! <= 0) return { notes: [], markSeen: null }
  const notes = releases
    .filter((r) => compareVersions(r.version, seen) === 1 && compareVersions(r.version, current)! <= 0)
    .sort((a, b) => compareVersions(b.version, a.version)!)
  return { notes, markSeen: current }
}

/** The note's text in the UI language (English for anything but Vietnamese). */
export function noteText(note: ReleaseNote, locale: string): string {
  return locale === 'vi' ? note.vi : note.en
}
