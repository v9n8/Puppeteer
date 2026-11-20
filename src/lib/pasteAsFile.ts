// Long pastes as a `.txt` attachment (#16, RT-26). The project setting is stored by the
// backend (`set_project_paste_as_file`, RT-25); here only the pure rules the composer and
// the project page share: when a paste becomes a file, and what the length field accepts.
//
// Length = `String.length` (UTF-16 code units) of the pasted chunk alone, not of the
// whole draft after the paste (user's decision, 2026-11-20).

/** A project's "long paste as a file" setting, as the backend sends it. */
export interface PasteAsFileSetting {
  enabled: boolean
  /** A paste longer than this becomes a file; a whole number from 1. */
  max_chars: number
}

/** Same as the backend's `DEFAULT_MAX_CHARS` and migration 0028. */
export const PASTE_DEFAULT_MAX_CHARS = 2000
/** Same as the backend's `MAX_MAX_CHARS`. */
export const PASTE_MAX_MAX_CHARS = 10_000_000

export const DEFAULT_PASTE_AS_FILE: PasteAsFileSetting = { enabled: true, max_chars: PASTE_DEFAULT_MAX_CHARS }

function validMax(n: unknown): n is number {
  return typeof n === 'number' && Number.isInteger(n) && n >= 1 && n <= PASTE_MAX_MAX_CHARS
}

/** The setting in force: the backend's default for a missing one or a bad length. */
export function pasteSetting(raw: Partial<PasteAsFileSetting> | null | undefined): PasteAsFileSetting {
  return {
    enabled: typeof raw?.enabled === 'boolean' ? raw.enabled : DEFAULT_PASTE_AS_FILE.enabled,
    max_chars: validMax(raw?.max_chars) ? raw.max_chars : PASTE_DEFAULT_MAX_CHARS,
  }
}

/** True when `text` (the pasted chunk) is saved to a file instead of going into the draft. */
export function shouldPasteAsFile(text: string, raw: Partial<PasteAsFileSetting> | null | undefined): boolean {
  const setting = pasteSetting(raw)
  return setting.enabled && text.length > setting.max_chars
}

/**
 * What the length field holds as a threshold: a whole number from 1 to
 * `PASTE_MAX_MAX_CHARS`, or null (empty, a fraction, a sign, letters, out of range).
 */
export function parseMaxChars(input: string | number | null | undefined): number | null {
  const s = String(input ?? '').trim()
  if (!/^\d+$/.test(s)) return null
  const n = Number(s)
  return validMax(n) ? n : null
}

/** The file name of a saved paste's path (either separator). */
export function pasteFileName(path: string): string {
  return path.split(/[/\\]/).pop() || path
}
