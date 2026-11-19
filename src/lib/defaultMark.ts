// A select of session defaults names a real value, never "follow the global
// setting"; a ★ marks the option that is the default in force (review v3, Q3).
// Pure, so tests cover it.

interface Row {
  value: string
  header?: boolean
}

/** `options` with ★ (`isDefault`) on the first selectable row whose value is `value`. */
export function markDefault<T extends Row>(options: readonly T[], value: string): (T & { isDefault?: boolean })[] {
  let marked = false
  return options.map((o) => {
    if (marked || o.header || o.value !== value) return o
    marked = true
    return { ...o, isDefault: true }
  })
}

/**
 * A project's default-for-new-sessions select: it shows the project's saved
 * value, else the global one, and ★ marks the global one. A '' row ("engine
 * decides") stays only while it is the global value: saving '' clears the
 * field, so the project then follows the global setting.
 */
export function projectDefaultSelect<T extends Row>(
  options: readonly T[],
  own: string | null | undefined,
  global: string,
): { value: string; options: (T & { isDefault?: boolean })[] } {
  return {
    value: own?.trim() || global,
    options: markDefault(options.filter((o) => o.value !== '' || global === ''), global),
  }
}
