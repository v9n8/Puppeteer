<script setup lang="ts">
// Project page › Session defaults (UX-19): the engine, model of each engine,
// permission mode, effort and response style a new session of this project
// starts with, and the project's Claude account. A field the project never saved
// uses Settings › Defaults for new sessions: each select shows the value in force
// and ★ marks the global one; picking any option saves it (review v3, Q3). A
// session that already ran keeps its own engine, model, effort and style.
// Below: long pastes as a `.txt` attachment (RT-26), stored apart from the defaults.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink } from 'vue-router'
import { ClipboardPaste, ShieldAlert, SlidersHorizontal } from 'lucide-vue-next'
import { AppSelect, Card, InfoTip, Input, ToggleSwitch } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { useProjectsStore, type Project } from '@/stores/projects'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useResponseStylesStore } from '@/stores/responseStyles'
import {
  EFFORT_LEVELS, MODEL_OPTIONS, effortLevelsFor, permissionModeOptions, withCurrentModel, type SelectOption,
} from '@/lib/engineOptions'
import { composerDefaults, composerStyleChoices, type ProjectSessionDefaults } from '@/lib/composerChips'
import { defaultEffortChoices } from '@/lib/sessionDefaults'
import { NO_STYLE, shownStyles } from '@/lib/responseStyles'
import { projectDefaultSelect } from '@/lib/defaultMark'
import { PASTE_MAX_MAX_CHARS, parseMaxChars, pasteSetting, type PasteAsFileSetting } from '@/lib/pasteAsFile'
import { IS_MAC } from '@/lib/appMenu'

const props = defineProps<{ project: Project }>()

const { t } = useI18n()
const { toast } = useToast()
const projectStore = useProjectsStore()
const appSettings = useAppSettingsStore()
const claudeStore = useClaudeAccountsStore()
const modelCatalog = useModelCatalogStore()
const styles = useResponseStylesStore()

modelCatalog.ensureLoaded().catch(() => {})
if (!claudeStore.accounts.length) claudeStore.fetch().catch(() => {})
// The styles usable here: global ones and this project's own.
watch(() => props.project.id, (id) => {
  styles.load(id).catch(() => {})
}, { immediate: true })

const own = computed<ProjectSessionDefaults>(() => props.project.session_defaults ?? {})
const global = computed(() => composerDefaults({ settings: appSettings.settings }))
const effective = computed(() => composerDefaults({ settings: appSettings.settings, project: own.value }))

const saving = ref(false)
async function save(field: keyof ProjectSessionDefaults, value: string) {
  if (saving.value || (own.value[field] ?? '') === value) return
  saving.value = true
  try {
    await projectStore.setSessionDefaults(props.project.id, { ...own.value, [field]: value || null })
    toast.success(t('projectDetail.sessionDefaults.saved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    saving.value = false
  }
}

// ── Engine and models ──
const engineSelect = computed(() => projectDefaultSelect<SelectOption>([
  { value: 'claude', label: 'Claude' },
  { value: 'codex', label: 'Codex' },
], own.value.engine, global.value.engine))
// '' = the engine decides; listed only while it is the global value.
const modelDefaultOption = (): SelectOption => ({ value: '', label: t('settings.ai.modelDefault') })
// Models turned off in Settings › Model or in Project › Model are left out; the
// model shown stays, marked.
const claudeModelSelect = computed(() => {
  const shown = own.value.model_claude || global.value.models.claude
  return projectDefaultSelect([
    modelDefaultOption(),
    ...modelCatalog.enabledOnly(
      'claude',
      modelCatalog.withValidation(withCurrentModel(MODEL_OPTIONS.claude.filter((o) => o.value), shown)),
      shown,
      props.project.id,
    ),
  ], own.value.model_claude, global.value.models.claude)
})
const codexModelSelect = computed(() => {
  const shown = own.value.model_codex || global.value.models.codex
  return projectDefaultSelect([
    modelDefaultOption(),
    ...modelCatalog.enabledOnly(
      'codex',
      withCurrentModel(modelCatalog.codexOptions(MODEL_OPTIONS.codex).filter((o) => o.value), shown),
      shown,
      props.project.id,
    ),
  ], own.value.model_codex, global.value.models.codex)
})

// ── Permission mode ──
const permissionSelect = computed(() =>
  projectDefaultSelect(permissionModeOptions(t), own.value.permission_mode, global.value.permissionMode),
)

// ── Effort: the levels of this project's engine + default model ──
const effortLevels = computed(() => {
  const engine = effective.value.engine
  const model = engine === 'codex' ? effective.value.models.codex : effective.value.models.claude
  return effortLevelsFor(engine, model, { codex: modelCatalog.codexModels })
})
const effortLabel = (level: string) =>
  (EFFORT_LEVELS as readonly string[]).includes(level) ? t(`settings.ai.effort.${level}`) : level
const effortSelect = computed(() =>
  projectDefaultSelect<SelectOption>(
    defaultEffortChoices(effortLevels.value, own.value.effort || global.value.effort).map((c) => {
      if (!c.value) return { value: '', label: t('settings.ai.effortEngineDefault') }
      const label = effortLabel(c.value)
      return c.supported ? { value: c.value, label } : { value: c.value, label, description: t('settings.ai.effortUnsupported') }
    }),
    own.value.effort,
    global.value.effort,
  ),
)

// ── Response style: the global styles, then this project's ──
const shown = computed(() => shownStyles(styles.styles, t))
function styleName(key: string): string {
  if (!key || key === NO_STYLE) return t('settings.ai.styleNone')
  const found = shown.value.find((s) => s.id === key)
  return found ? found.name : styles.loaded ? t('settings.ai.styleMissing') : '…'
}
// A global '' and the saved "None" ('default') both mean no style.
const globalStyle = computed(() => global.value.style || NO_STYLE)
const styleSelect = computed(() => {
  const out: (SelectOption & { header?: boolean })[] = []
  let group = ''
  const heading = (id: string, label: string) => {
    if (group === id) return
    group = id
    out.push({ value: `__group:${id}`, label, header: true })
  }
  for (const c of composerStyleChoices(shown.value, props.project.id, own.value.style || globalStyle.value)) {
    if (c.kind === 'none') {
      out.push({ value: c.value, label: t('settings.ai.styleNone'), description: t('settings.ai.styleNoneHint') })
    } else if (c.kind === 'mine' || c.kind === 'project') {
      heading(c.kind, c.kind === 'mine' ? t('responseStyles.mineTitle') : t('responseStyles.projectStylesTitle'))
      out.push(c.description ? { value: c.value, label: c.name, description: c.description } : { value: c.value, label: c.name })
    } else {
      out.push({ value: c.value, label: styleName(c.value) })
    }
  }
  return projectDefaultSelect(out, own.value.style, globalStyle.value)
})

// ── Long pastes as a file (RT-26): a whole number from 1, never empty ──
const paste = computed(() => pasteSetting(props.project.paste_as_file))
const pasteDraft = ref('')
watch(() => paste.value.max_chars, (n) => { pasteDraft.value = String(n) }, { immediate: true })
const pasteInvalid = computed(() => parseMaxChars(pasteDraft.value) === null)
const pasteSaving = ref(false)
async function savePaste(next: PasteAsFileSetting) {
  if (pasteSaving.value) return
  if (next.enabled === paste.value.enabled && next.max_chars === paste.value.max_chars) return
  pasteSaving.value = true
  try {
    await projectStore.setPasteAsFile(props.project.id, next)
    toast.success(t('projectDetail.pasteAsFile.saved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    pasteSaving.value = false
  }
}
function togglePaste() {
  void savePaste({ ...paste.value, enabled: !paste.value.enabled })
}
function commitPasteMax() {
  const n = parseMaxChars(pasteDraft.value)
  if (n === null) return // the field shows why; nothing saved
  void savePaste({ ...paste.value, max_chars: n })
}

// ── Claude account (one per project; falls back to the default account) ──
const linkedClaudeAccount = computed(
  () => claudeStore.accounts.find((a) => a.id === props.project.claude_account_id) ?? null,
)
const claudeAccountOptions = computed<SelectOption[]>(() => {
  const def = claudeStore.accounts.find((a) => a.is_default)
  return [
    { value: '', label: def ? t('projectDetail.claudeDefaultOption', { label: def.label }) : t('projectDetail.sessionDefaults.noAccount') },
    ...claudeStore.accounts.map((a) => ({ value: a.id, label: a.label })),
  ]
})
async function selectClaudeAccount(accountId: string) {
  try {
    await projectStore.setProjectClaudeAccount(props.project.id, accountId || null)
    toast.success(t('projectDetail.toastClaudeLinked'))
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <div class="max-w-lg space-y-4">
    <Card body-class="p-4 space-y-4">
      <template #header>
        <SlidersHorizontal class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.sessionDefaults.title') }}</span>
        <InfoTip :text="`${t('projectDetail.sessionDefaults.hint')} ${t('settings.ai.sessionDefaults')}.`" />
        <RouterLink to="/settings?section=ai" class="ml-auto inline-flex min-h-7 items-center px-1 text-2xs text-primary hover:underline">{{ t('settings.ai.sessionDefaults') }}</RouterLink>
      </template>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultEngine') }}</label>
        <AppSelect size="sm" :model-value="engineSelect.value" :options="engineSelect.options" @update:model-value="save('engine', $event)" />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
          {{ t('projectDetail.sessionDefaults.claudeAccount') }}<InfoTip :text="`${t('projectDetail.claudeAccountHint')} ${t('projectDetail.settings')}.`" />
        </label>
        <div v-if="claudeStore.accounts.length === 0" class="text-2xs text-muted-foreground">
          {{ t('projectDetail.noClaudeAccounts') }}
          <RouterLink to="/settings?section=claude" class="text-primary hover:underline">{{ t('projectDetail.addInSettings') }}</RouterLink>
          {{ t('projectDetail.toLinkHere') }}
        </div>
        <AppSelect
          v-else
          size="sm"
          :model-value="project.claude_account_id ?? ''"
          :options="claudeAccountOptions"
          :placeholder="t('projectDetail.selectAccountPlaceholder')"
          @update:model-value="selectClaudeAccount"
        />
        <div v-if="linkedClaudeAccount" class="text-2xs text-muted-foreground">
          {{ t('settings.claude.status.' + linkedClaudeAccount.status) }}
          <span v-if="linkedClaudeAccount.email"> · {{ linkedClaudeAccount.email }}</span>
        </div>
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultClaudeModel') }}</label>
        <AppSelect size="sm" :model-value="claudeModelSelect.value" :options="claudeModelSelect.options" @update:model-value="save('model_claude', $event)" />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultCodexModel') }}</label>
        <AppSelect size="sm" :model-value="codexModelSelect.value" :options="codexModelSelect.options" @update:model-value="save('model_codex', $event)" />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
          <ShieldAlert class="h-3 w-3" :stroke-width="1.75" />{{ t('settings.ai.defaultPermissionMode') }}
        </label>
        <AppSelect size="sm" :model-value="permissionSelect.value" :options="permissionSelect.options" @update:model-value="save('permission_mode', $event)" />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
          {{ t('settings.ai.defaultEffort') }}<InfoTip :text="t('settings.ai.effortHint')" />
        </label>
        <template v-if="effortLevels.length || own.effort">
          <AppSelect size="sm" :model-value="effortSelect.value" :options="effortSelect.options" @update:model-value="save('effort', $event)" />
        </template>
        <p v-else class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.ai.effortNotTaken') }}</p>
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultStyle') }}</label>
        <AppSelect size="sm" :model-value="styleSelect.value" :options="styleSelect.options" @update:model-value="save('style', $event)" />
      </div>
    </Card>

    <Card body-class="p-4 space-y-4">
      <template #header>
        <ClipboardPaste class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.pasteAsFile.title') }}</span>
        <InfoTip :text="t('projectDetail.pasteAsFile.hint')" />
      </template>
      <div class="flex items-center justify-between gap-3">
        <span class="flex items-center gap-1.5 text-xs">
          {{ t('projectDetail.pasteAsFile.enabled') }}<InfoTip :text="t('projectDetail.pasteAsFile.shortcutHint', { key: IS_MAC ? '⌘⇧V' : 'Ctrl+Shift+V' })" />
        </span>
        <ToggleSwitch
          size="md"
          :checked="paste.enabled"
          :loading="pasteSaving"
          :label="t('projectDetail.pasteAsFile.enabled')"
          @toggle="togglePaste"
        />
      </div>
      <div class="space-y-1.5">
        <label for="paste-as-file-max" class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('projectDetail.pasteAsFile.maxChars') }}</label>
        <Input
          id="paste-as-file-max"
          v-model="pasteDraft"
          type="number"
          min="1"
          :max="PASTE_MAX_MAX_CHARS"
          step="1"
          required
          :disabled="!paste.enabled"
          :aria-invalid="pasteInvalid"
          @change="commitPasteMax"
          @keydown.enter="commitPasteMax"
        />
        <p v-if="pasteInvalid" class="text-2xs text-destructive">{{ t('projectDetail.pasteAsFile.invalid', { max: PASTE_MAX_MAX_CHARS }) }}</p>
      </div>
    </Card>

  </div>
</template>
