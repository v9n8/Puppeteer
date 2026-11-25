// After Settings › Defaults for new sessions saves a new engine or default model:
// a toast names the projects that follow it but whose own model list turns it
// off. Nothing is changed; the user fixes one side or the other.
import { useI18n } from 'vue-i18n'
import { useToast } from '@/composables/useToast'
import { useProjectsStore } from '@/stores/projects'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { FOLLOWED_MODEL_KEYS, blockedFollowers, namesShown } from '@/lib/globalDefaultWarning'
import type { DefaultSettings } from '@/lib/composerChips'

/** Call during setup; the returned function runs after a save of `keys`. */
export function useWarnBlockedFollowers() {
  const { t } = useI18n()
  const { toast } = useToast()
  const projects = useProjectsStore()
  const modelCatalog = useModelCatalogStore()

  return async function warnBlockedFollowers(keys: readonly string[], settings: DefaultSettings) {
    if (!keys.some((k) => FOLLOWED_MODEL_KEYS.includes(k))) return
    // The one round trip, only while the project list is not loaded yet.
    if (!projects.projects.length) await projects.fetchProjects()
    const blocked = blockedFollowers(projects.projects, settings, (id) => modelCatalog.enabledFor(id), keys)
    if (!blocked.length) return
    const shown = namesShown(blocked.map((p) => p.name))
    const names = shown.more ? t('settings.ai.followersMore', { names: shown.names, count: shown.more }) : shown.names
    toast.info(t('settings.ai.followersBlocked', { names }), 8000)
  }
}
