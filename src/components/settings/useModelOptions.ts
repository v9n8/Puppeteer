// Claude / Codex model choices shared by Settings → "Defaults for new sessions"
// (the default model of each engine, Claude's fallback model) and Settings →
// "Translation" (the translate model), so the selects offer the same list.
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { MODEL_OPTIONS, withCurrentModel, type SelectOption } from '@/lib/engineOptions'
import { useSettingsForm } from './useSettingsForm'

export function useModelOptions() {
  const { t } = useI18n()
  const modelCatalog = useModelCatalogStore()
  const { settings } = useSettingsForm()

  // The pinned Claude and Codex tables, shared with the composer and the remote
  // controller. Only the leading "Default" placeholder is translated.
  const withDefaultLabel = (opts: SelectOption[]) =>
    opts.map((o) => (o.value === '' ? { ...o, label: t('settings.ai.modelDefault') } : o))
  const CLAUDE_MODEL_OPTIONS = withDefaultLabel(MODEL_OPTIONS.claude)
  const CODEX_MODEL_OPTIONS = withDefaultLabel(MODEL_OPTIONS.codex)

  // Claude model choices = the pinned table, annotated with the last validation
  // sweep, plus whatever is already saved (so an id dropped from the table — or a
  // family alias from an older build — is never silently rewritten), without the
  // models turned off in Settings › Model (the saved one stays, marked).
  function claudeOptionsFor(current: string) {
    return modelCatalog.enabledOnly(
      'claude',
      modelCatalog.withValidation(withCurrentModel(CLAUDE_MODEL_OPTIONS, current)),
      current,
    )
  }
  // Codex model choices = curated fallback, replaced by the CLI-refreshed cache
  // (`codex debug models`) once the user scans in Settings › Model.
  function codexOptionsFor(current: string) {
    return modelCatalog.enabledOnly('codex', modelCatalog.codexOptions(CODEX_MODEL_OPTIONS), current)
  }

  const claudeModelOptions = computed(() => claudeOptionsFor(settings.value.claude_model))
  // Fallback model choices (AC-17): off, the `sonnet` alias (the default, so it
  // follows the current Sonnet), then the same pinned table and the saved pick.
  const claudeFallbackOptions = computed(() => {
    const current = settings.value.claude_fallback_model
    return modelCatalog.enabledOnly(
      'claude',
      modelCatalog.withValidation(
        withCurrentModel(
          [
            { value: '', label: t('settings.ai.fallbackOff') },
            { value: 'sonnet', label: 'sonnet', description: t('settings.ai.fallbackSonnetAlias') },
            ...CLAUDE_MODEL_OPTIONS.filter((o) => o.value !== ''),
          ],
          current,
        ),
      ),
      current,
    )
  })
  const codexModelOptions = computed(() => codexOptionsFor(settings.value.codex_model))
  // The translate model follows the translate engine.
  const translateModelOptions = computed(() =>
    settings.value.translate_engine === 'codex'
      ? codexOptionsFor(settings.value.translate_model)
      : claudeOptionsFor(settings.value.translate_model),
  )

  onMounted(() => {
    // Hydrate the persisted model caches (no discovery — refresh is manual).
    modelCatalog.ensureLoaded().catch(() => {})
  })

  return { claudeModelOptions, claudeFallbackOptions, codexModelOptions, translateModelOptions }
}
