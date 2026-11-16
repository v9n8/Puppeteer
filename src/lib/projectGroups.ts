export type GroupKind = 'skill' | 'rule'

/** Badge tones a group can be tinted with. */
type GroupColor = 'primary' | 'info' | 'success' | 'warning' | 'neutral'

/** A group as seen from one project: how much of it is applied, and whether the project runs it. */
export interface ProjectGroupState {
  group_id: string
  name: string
  description: string
  color: GroupColor | null
  member_count: number
  applied_count: number
  linked: boolean
  state: 'on' | 'mixed' | 'off'
}

interface GroupItemFailure {
  item_id: string
  item_name: string
  error: string
}

export interface GroupApplyOutcome {
  applied: number
  failures: GroupItemFailure[]
}

export interface GroupDisableOutcome {
  removed: number
  kept_manual: number
  kept_other_group: number
  failures: GroupItemFailure[]
}
