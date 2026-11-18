// Shared look of the list-detail sidebars (ListDetailSidebar, the Notes /
// Todos ItemSidebar, Projects, Library…): the pane shell, 6px rows on a solid
// accent when hovered or selected (a primary bar marks the selected one), and
// a two-level text scale (title / secondary line).

/** The list pane (left column): card surface, solid divider. Callers add the width. */
export const listPaneClass = 'flex shrink-0 flex-col border-r border-border bg-card'

/** The pane's top block (search, filters, add). Callers add the padding. */
export const listPaneHeadClass = 'shrink-0 border-b border-border'

/** Wrapper of one group: a little air between folders. */
export const groupClass = 'mb-3 last:mb-0'

/** Folder header row. */
export const groupHeaderClass = 'group/folder flex min-h-7.5 items-center gap-1.5 rounded-sm pl-1.5 pr-1 text-xs font-semibold hover:bg-accent'

/** The rows of one group: 2px apart, no dividers. */
export const rowListClass = 'mt-1 space-y-0.5'

const rowBase = 'relative flex w-full items-start gap-2 rounded-sm px-2 py-1.5 transition-colors'

export function rowClass(selected: boolean): string {
  return selected
    ? `${rowBase} bg-accent text-foreground before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary`
    : `${rowBase} text-foreground/85 hover:bg-accent`
}

/** A row's main line. */
export const rowTitleClass = 'text-sm font-medium leading-snug'

/** A row's secondary line (description, preview, user@host…). */
export const rowSubClass = 'mt-1 truncate text-2xs text-muted-foreground/70'
