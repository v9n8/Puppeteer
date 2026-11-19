// The rows the Settings search (lib/settingsSearch) can open, per section.
//
// Adding a section or a row to Settings:
//   1. a new section in SETTINGS_SECTIONS needs a key here too (vue-tsc fails until it has one);
//   2. a new row: add it here AND put its id on the screen — `search-id="<id>"` on a
//      SettingsRow, or `data-settings-id="<id>"` on any other element. The test
//      (__tests__/settingsSearch.test.mjs) fails for an id the screens never carry,
//      and for a labelKey / keywordsKey missing in en or vi;
//   3. `labelKey` is a full i18n key (any namespace); `keywordsKey` points at a comma
//      list of synonyms under settings.searchKeywords; `tab` is the tab of the section
//      that holds the row (see TAB_LABEL_KEY in settingsSearch.ts).
// Every section is also found by its own name, without being listed as a row.
import type { SettingsSectionId } from './settingsSections.ts'

export interface SearchRow {
  /** Unique across Settings; the row's `search-id` / `data-settings-id`. */
  id: string
  labelKey: string
  tab?: string
  keywordsKey?: string
}

const kw = (id: string) => `settings.searchKeywords.${id}`

export const SETTINGS_SEARCH_ROWS: Record<SettingsSectionId, SearchRow[]> = {
  general: [
    { id: 'general-mode', labelKey: 'settings.general.mode', keywordsKey: kw('theme') },
    { id: 'general-colorTheme', labelKey: 'settings.general.colorTheme', keywordsKey: kw('theme') },
    { id: 'general-rowDensity', labelKey: 'settings.general.rowDensity' },
    { id: 'general-font', labelKey: 'settings.general.font', keywordsKey: kw('font') },
    { id: 'general-weight', labelKey: 'settings.general.weight', keywordsKey: kw('font') },
    { id: 'general-smoothing', labelKey: 'settings.general.smoothing', keywordsKey: kw('font') },
    { id: 'general-appFontSize', labelKey: 'settings.general.appFontSize', keywordsKey: kw('font') },
    { id: 'general-chatFontSize', labelKey: 'settings.general.chatFontSize', keywordsKey: kw('font') },
    { id: 'general-monoFont', labelKey: 'settings.general.monoFont', keywordsKey: kw('font') },
    { id: 'general-language', labelKey: 'settings.language', keywordsKey: kw('language') },
    { id: 'general-terminalApp', labelKey: 'settings.general.terminalApp' },
    { id: 'general-launchAtLogin', labelKey: 'settings.general.launchAtLogin' },
    { id: 'general-quitPolicy', labelKey: 'settings.general.quitPolicy' },
  ],
  flags: [
    { id: 'flags-list', labelKey: 'sessionFlags.settings.title', keywordsKey: kw('flags') },
  ],
  features: [
    { id: 'features-modules', labelKey: 'features.settings.title', keywordsKey: kw('features') },
  ],
  focus: [
    { id: 'focus-workHours', labelKey: 'focus.settings.workHours' },
    { id: 'focus-workDays', labelKey: 'focus.settings.workDays' },
  ],
  shortcuts: [
    { id: 'shortcuts-palette', labelKey: 'settings.shortcuts.tabs.palette', tab: 'palette', keywordsKey: kw('palette') },
    { id: 'shortcuts-keymap', labelKey: 'settings.shortcuts.tabs.keymap', tab: 'keymap', keywordsKey: kw('keymap') },
  ],
  engines: [
    { id: 'engines-claudePath', labelKey: 'settings.engine.claudePath', keywordsKey: kw('enginePath') },
    { id: 'engines-codexPath', labelKey: 'settings.engine.codexPath', keywordsKey: kw('enginePath') },
  ],
  model: [
    { id: 'model-allowed', labelKey: 'settings.search.rows.modelAllowed', keywordsKey: kw('model') },
  ],
  ai: [
    { id: 'ai-defaultEngine', labelKey: 'settings.ai.defaultEngine' },
    { id: 'ai-defaultClaudeModel', labelKey: 'settings.ai.defaultClaudeModel', keywordsKey: kw('model') },
    { id: 'ai-claudeFallbackModel', labelKey: 'settings.ai.claudeFallbackModel', keywordsKey: kw('model') },
    { id: 'ai-defaultCodexModel', labelKey: 'settings.ai.defaultCodexModel', keywordsKey: kw('model') },
    { id: 'ai-defaultPermissionMode', labelKey: 'settings.ai.defaultPermissionMode' },
    { id: 'ai-defaultEffort', labelKey: 'settings.ai.defaultEffort' },
    { id: 'ai-defaultStyle', labelKey: 'settings.ai.defaultStyle' },
    { id: 'ai-defaultMaxWorkers', labelKey: 'settings.ai.defaultMaxWorkers' },
    { id: 'ai-workerEffort', labelKey: 'settings.ai.workerEffort' },
    { id: 'ai-subagentModel', labelKey: 'settings.ai.subagentModel', keywordsKey: kw('model') },
    { id: 'ai-subagentEffort', labelKey: 'settings.ai.subagentEffort' },
  ],
  styles: [
    { id: 'styles-mine', labelKey: 'responseStyles.mineTitle' },
  ],
  translate: [
    { id: 'translate-engine', labelKey: 'settings.translate.engine', keywordsKey: kw('translate') },
    { id: 'translate-model', labelKey: 'settings.translate.model', keywordsKey: kw('translate') },
    { id: 'translate-targetLanguage', labelKey: 'settings.translate.targetLanguage', keywordsKey: kw('translate') },
    { id: 'translate-style', labelKey: 'settings.translate.style', keywordsKey: kw('translate') },
  ],
  memory: [
    { id: 'memory-file', labelKey: 'memory.file', keywordsKey: kw('memory') },
  ],
  limits: [
    { id: 'limits-warnAt', labelKey: 'settings.sessions.warnAt', tab: 'context', keywordsKey: kw('limits') },
    { id: 'limits-limitOverride', labelKey: 'settings.sessions.limitOverride', tab: 'context', keywordsKey: kw('limits') },
    { id: 'limits-quotaWarnAt', labelKey: 'settings.sessions.quotaWarnAt', tab: 'quota', keywordsKey: kw('quota') },
    { id: 'limits-block5h', labelKey: 'settings.sessions.block5h', tab: 'quota', keywordsKey: kw('quota') },
    { id: 'limits-blockWeekly', labelKey: 'settings.sessions.blockWeekly', tab: 'quota', keywordsKey: kw('quota') },
    { id: 'limits-stopRunning', labelKey: 'settings.sessions.stopRunning', tab: 'quota', keywordsKey: kw('quota') },
    { id: 'limits-unattendedMaxTurns', labelKey: 'settings.sessions.unattendedMaxTurns', tab: 'unattended', keywordsKey: kw('limits') },
    { id: 'limits-unattendedMaxMinutes', labelKey: 'settings.sessions.unattendedMaxMinutes', tab: 'unattended', keywordsKey: kw('limits') },
    { id: 'limits-notify', labelKey: 'settings.sessions.notifyTitle', tab: 'notify', keywordsKey: kw('notify') },
  ],
  routing: [
    { id: 'routing-policy', labelKey: 'routing.switches.policy', keywordsKey: kw('routing') },
    { id: 'routing-escalation', labelKey: 'routing.switches.escalation', keywordsKey: kw('routing') },
    { id: 'routing-autoUpdate', labelKey: 'routing.switches.autoUpdate', keywordsKey: kw('routing') },
    { id: 'routing-report', labelKey: 'routing.report.title', keywordsKey: kw('routing') },
    { id: 'routing-updateNow', labelKey: 'routing.update.now', keywordsKey: kw('routing') },
  ],
  connections: [
    { id: 'connections-list', labelKey: 'settings.connections.title', keywordsKey: kw('connections') },
    { id: 'connections-validateAll', labelKey: 'settings.connections.validateAll', keywordsKey: kw('connections') },
  ],
  mcp: [
    { id: 'mcp-servers', labelKey: 'settings.search.rows.mcpServers', keywordsKey: kw('mcp') },
  ],
  inbox: [
    { id: 'inbox-status', labelKey: 'settings.inboxApi.status', keywordsKey: kw('inbox') },
  ],
  devices: [
    { id: 'devices-relayUrl', labelKey: 'remote.settings.relayUrl', keywordsKey: kw('relay') },
    { id: 'devices-authToken', labelKey: 'remote.settings.authToken', keywordsKey: kw('relay') },
    { id: 'devices-deviceName', labelKey: 'remote.settings.deviceName' },
    { id: 'devices-masterPassword', labelKey: 'remote.settings.masterPassword' },
    { id: 'devices-checkRelay', labelKey: 'remote.settings.checkRelay', keywordsKey: kw('relay') },
    { id: 'devices-backup', labelKey: 'settings.backup.title', tab: 'backup', keywordsKey: kw('backup') },
  ],
  about: [
    { id: 'about-version', labelKey: 'settings.search.rows.version', keywordsKey: kw('version') },
    { id: 'about-updates', labelKey: 'about.updates', keywordsKey: kw('updates') },
    { id: 'about-whatsNew', labelKey: 'about.whatsNew', keywordsKey: kw('whatsNew') },
  ],
}
