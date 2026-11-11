// Claude / Codex model choices shared by Settings → "Defaults for new sessions"
// (the default model of each engine, Claude's fallback model) and Settings →
// "Translation" (the translate model), so the selects offer the same list.
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { MODEL_OPTIONS, withCurrentModel } from '@/lib/engineOptions'
import { useSettingsForm } from './useSettingsForm'

export function useModelOptions() {
  const { t } = useI18n()
  const modelCatalog = useModelCatalogStore()
  const { settings } = useSettingsForm()

  // The pinned Claude table, shared with the composer and the remote controller.
  // Only the leading "Default" placeholder is translated; the rest are model ids.
  const CLAUDE_MODEL_OPTIONS = MODEL_OPTIONS.claude.map((o) =>
    o.value === '' ? { ...o, label: t('settings.ai.modelDefault') } : o,
  )
  const CODEX_MODEL_OPTIONS = [
    { value: '', label: t('settings.ai.modelDefault') },
    { value: 'gpt-5.5', label: 'gpt-5.5' },
    { value: 'gpt-5.4', label: 'gpt-5.4' },
    { value: 'gpt-5.3-codex', label: 'gpt-5.3-codex' },
    { value: 'gpt-5.2-codex', label: 'gpt-5.2-codex' },
    { value: 'gpt-5.1-codex-mini', label: 'gpt-5.1-codex-mini' },
  ]

  // Claude model choices = the pinned table, annotated with the last validation
  // sweep, plus whatever is already saved (so an id dropped from the table — or a
  // family alias from an older build — is never silently rewritten).
  const claudeModelOptions = computed(() =>
    modelCatalog.withValidation(
      withCurrentModel(CLAUDE_MODEL_OPTIONS, settings.value.claude_model),
    ),
  )
  // Fallback model choices (AC-17): off, the `sonnet` alias (the default, so it
  // follows the current Sonnet), then the same pinned table and the saved pick.
  const claudeFallbackOptions = computed(() =>
    modelCatalog.withValidation(
      withCurrentModel(
        [
          { value: '', label: t('settings.ai.fallbackOff') },
          { value: 'sonnet', label: 'sonnet', description: t('settings.ai.fallbackSonnetAlias') },
          ...CLAUDE_MODEL_OPTIONS.filter((o) => o.value !== ''),
        ],
        settings.value.claude_fallback_model,
      ),
    ),
  )
  // Codex model choices = curated fallback, replaced by the CLI-refreshed cache
  // (`codex debug models`) once the user refreshes in Settings.
  const codexModelOptions = computed(() => modelCatalog.codexOptions(CODEX_MODEL_OPTIONS))

  onMounted(() => {
    // Hydrate the persisted model caches (no discovery — refresh is manual).
    modelCatalog.ensureLoaded().catch(() => {})
  })

  return { claudeModelOptions, claudeFallbackOptions, codexModelOptions }
}

/** Format a model-cache timestamp for the "last refreshed" hint. */
export function fmtRefreshed(iso: string | null | undefined): string {
  if (!iso) return ''
  const d = new Date(iso)
  return isNaN(d.getTime()) ? '' : d.toLocaleString()
}
