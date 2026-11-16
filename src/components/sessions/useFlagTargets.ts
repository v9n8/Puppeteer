// Where the flag menus lead: Settings › Flag and the open Project's Flag tab.
// An entry whose screen does not exist yet is not offered (V3-19 AC-12), so the
// `can…` flags drive `v-if`; they turn on by themselves once the screens ship.
import { useRouter } from 'vue-router'
import { projectPageQuery } from '@/lib/projectSections'
import { resolveSettingsPlace, settingsLocation } from '@/lib/settingsSections'
import { hasProjectSection, hasSettingsSection } from '@/lib/settingsTargets'

export function useFlagTargets() {
  const router = useRouter()
  const canManage = hasSettingsSection('flags')
  const canOpenProjectFlags = hasProjectSection('flags')

  function openManage() {
    const place = resolveSettingsPlace('flags')
    if (place) router.push(settingsLocation(place))
  }

  function openProjectFlags(projectId: string) {
    router.push({ path: '/projects', query: projectPageQuery(projectId, 'flags') })
  }

  return { canManage, canOpenProjectFlags, openManage, openProjectFlags }
}
