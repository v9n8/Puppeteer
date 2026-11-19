// The composer's account menu (UR-55): names only, ✓ on the account the
// Session runs on, ★ on the account a run gets when it picks none, and the
// expired logins locked. Pure, so tests cover it.

/** One account the menu offers (useRunComposer `accountChoices`). */
export interface AccountChoice {
  /** The id to save; '' is the shared Codex login; null = the one login there is, not a choice. */
  value: string | null
  label: string
  /** The usage row key (`claude:<id>`, `codex`, …), null when it has none. */
  usageKey: string | null
}

export interface AccountMenuRow extends AccountChoice {
  isDefault: boolean
  isCurrent: boolean
  expired: boolean
  /** What a click saves: '' = back to the default; null = nothing to change. */
  pick: string | null
}

/**
 * `defaultId`: the account a run with no pick runs on (Claude: the Project's,
 * else the global default; Codex: '' = the shared login). `selectedId`: the
 * run's own pick, '' when it has none. A run with no pick sits on the ★ row,
 * so clicking that row changes nothing; from another account it saves '' (the
 * run follows the default again, as the old "Default" row did).
 */
export function accountMenuRows(
  choices: readonly AccountChoice[],
  defaultId: string,
  selectedId: string,
  isExpired: (usageKey: string | null) => boolean = () => false,
): AccountMenuRow[] {
  const current = selectedId || defaultId
  return choices.map((c) => {
    const isDefault = c.value !== null && c.value === defaultId
    const isCurrent = c.value === null || c.value === current
    const expired = isExpired(c.usageKey)
    const pick = isCurrent || expired ? null : isDefault ? '' : c.value
    return { ...c, isDefault, isCurrent, expired, pick }
  })
}
