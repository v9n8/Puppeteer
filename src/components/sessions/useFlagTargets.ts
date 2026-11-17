// Where the flag menus lead: Settings › Flag and the open Project's Flag tab.
// An entry whose screen does not exist yet is not offered (V3-19 AC-12), so the
// `can…` flags drive `v-if`; they turn on by themselves once the screens ship.
import { useRouter } from 'vue-router'
import { emit as emitEvent } from '@tauri-apps/api/event'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute } from '@/lib/detachedScreens'
import { IS_SETTINGS_WINDOW } from '@/lib/windowMode'
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
    const query = projectPageQuery(projectId, 'flags')
    // The Settings window has no router view: ask the main window to open the page.
    if (IS_SETTINGS_WINDOW) {
      const path = `/projects?${new URLSearchParams(query)}`
      void emitEvent(SCREEN_OPEN_ROUTE, { path } satisfies ScreenOpenRoute)
      return
    }
    router.push({ path: '/projects', query })
  }

  return { canManage, canOpenProjectFlags, openManage, openProjectFlags }
}
