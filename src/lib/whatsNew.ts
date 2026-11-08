// "What's new" popup (`app.whats_new`, components/WhatsNewModal.vue): the release
// notes bundled with the app and the pure decision of what to show at launch
// (tested in __tests__/whatsNew.test.mjs). Notes ship with the build, never
// fetched, so the popup works offline and opens no network path.

/** Settings row holding the newest version whose notes were offered. Not in a backup. */
export const SEEN_SETTING_KEY = 'whats_new_seen_version'

export type ChangeKind = 'added' | 'improved' | 'changed' | 'fixed'

export interface ReleaseNote {
  kind: ChangeKind
  en: string
  vi: string
}

export interface Release {
  /** Same string as `version` in tauri.conf.json (no leading `v`). */
  version: string
  /** `YYYY-MM-DD`. */
  date: string
  items: ReleaseNote[]
}

/**
 * Notes per release, newest first. Add the entry in the commit that bumps the
 * version: a release without one shows no popup.
 */
export const RELEASES: Release[] = []

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
