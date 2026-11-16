// Which kind of webview am I?
//
// The answer is read ONCE, at module load — which happens during the initial
// import graph, before the router runs its first navigation and rewrites the URL.
// Reading `window.location.search` later (e.g. from a route component that mounts
// after navigation) can see the query already stripped, so anything that needs to
// know it is a session pop-out must import from here rather than re-parse the URL.
const params = new URLSearchParams(window.location.search)

/** This webview is a standalone single-run pop-out (see lib/sessionWindow). */
export const IS_SESSION_WINDOW = params.get('sessionWindow') === '1'

/** The project / run the session window was opened for (null in every other window). */
export const SESSION_WINDOW_PROJECT_ID = params.get('projectId')
export const SESSION_WINDOW_RUN_ID = params.get('runId')

// Pop-out windows load the same SPA with a query flag; render a bare,
// chrome-less host (no sidebar / nav / background work) in those cases.
export const IS_FILE_WINDOW = params.get('fileWindow') === '1'
export const IS_PERMISSION_WINDOW = params.get('permissionWindow') === '1'
export const IS_ITEM_WINDOW = params.get('itemWindow') === '1'
export const IS_ITEM_LIST_WINDOW = params.get('itemListWindow') === '1'
export const IS_SECRET_NOTES_WINDOW = params.get('secretNotesWindow') === '1'
export const IS_GANTT_WINDOW = params.get('ganttWindow') === '1'
export const IS_SLACK_WINDOW = params.get('slackWindow') === '1'
export const IS_PR_INBOX_WINDOW = params.get('prInboxWindow') === '1'
export const IS_CALENDAR_WINDOW = params.get('calendarWindow') === '1'
export const IS_FOCUS_WINDOW = params.get('focusWindow') === '1'
/**
 * A Todos list window opened before the Tasks screen replaced it
 * (`?itemListWindow=1&kind=todo`): it loads the Tasks window's screen instead.
 */
export const IS_LEGACY_TODO_LIST_WINDOW = params.get('itemListWindow') === '1' && params.get('kind') !== 'note'
/** What the Tasks window opens on (`view`, `project`, `id`; the old Todos window's `projectId` too). */
export const TASKS_WINDOW_QUERY: Readonly<Record<string, string>> =
  IS_FOCUS_WINDOW || IS_LEGACY_TODO_LIST_WINDOW
    ? { ...Object.fromEntries(params), ...(IS_LEGACY_TODO_LIST_WINDOW ? { view: 'list', project: params.get('projectId') ?? '' } : {}) }
    : {}
export const IS_FILE_TRANSFER_WINDOW = params.get('fileTransferWindow') === '1'
export const IS_SETTINGS_WINDOW = params.get('settingsWindow') === '1'
/** The place the Settings window opens on (`section`, `tab`, `type`; see lib/settingsWindow). */
export const SETTINGS_WINDOW_QUERY: Readonly<Record<string, string>> = IS_SETTINGS_WINDOW ? Object.fromEntries(params) : {}
// All pop-out kinds only need the theme applied; skip the main app's data work.
export const IS_POPOUT_WINDOW =
  IS_FILE_WINDOW ||
  IS_PERMISSION_WINDOW ||
  IS_ITEM_WINDOW ||
  IS_ITEM_LIST_WINDOW ||
  IS_SECRET_NOTES_WINDOW ||
  IS_GANTT_WINDOW ||
  IS_SLACK_WINDOW ||
  IS_PR_INBOX_WINDOW ||
  IS_CALENDAR_WINDOW ||
  IS_FOCUS_WINDOW ||
  IS_FILE_TRANSFER_WINDOW ||
  IS_SETTINGS_WINDOW
