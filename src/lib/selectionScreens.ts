// What the shared selection bar (components/SelectionToolbar.vue) offers on the
// reading screens that are not a session: translate the selection, or copy it
// as the markdown it was rendered from. None of them has one session to quote
// into — a Slack thread or a note may link several, a skill, a rule or the PR
// list none — so Quote stays off. RunView and the file window, which do have
// one, declare their own surfaces.
//
// Kept free of Vue / `@/` imports so it can be unit-tested with `node --test`.
import type { SelectionAllow } from './selectionSurface.ts'

export type ReadingScreen = 'skill' | 'rule' | 'item' | 'slackThread' | 'prInbox'

export const SCREEN_SELECTION: Readonly<Record<ReadingScreen, SelectionAllow>> = {
  skill: { translate: true, copyMd: true },
  rule: { translate: true, copyMd: true },
  item: { translate: true, copyMd: true },
  slackThread: { translate: true, copyMd: true },
  prInbox: { translate: true, copyMd: true },
}
