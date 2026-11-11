import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/lib/tauri'
import { NO_STYLE, type ResponseStyle, type ResponseStyleList } from '@/lib/responseStyles'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useProjectsStore } from '@/stores/projects'

/**
 * Response styles (UX-17, UX-36): every style is a row, the samples too.
 * `styles` holds the global ones plus, after `load(projectId)`, that project's own.
 */
export const useResponseStylesStore = defineStore('responseStyles', () => {
  const styles = ref<ResponseStyle[]>([])
  const loaded = ref(false)

  async function load(projectId?: string | null): Promise<ResponseStyleList> {
    const list = await invoke<ResponseStyleList>('list_response_styles', { projectId: projectId ?? null })
    styles.value = list.styles
    loaded.value = true
    return list
  }

  async function create(input: { projectId?: string | null; name: string; description: string; body: string }) {
    const style = await invoke<ResponseStyle>('create_response_style', {
      projectId: input.projectId ?? null,
      name: input.name,
      description: input.description,
      body: input.body,
    })
    styles.value = [...styles.value, style]
    return style
  }

  async function update(id: string, input: { name: string; description: string; body: string }) {
    const style = await invoke<ResponseStyle>('update_response_style', { id, ...input })
    styles.value = styles.value.map((s) => (s.id === id ? style : s))
    return style
  }

  async function remove(id: string) {
    await invoke('delete_response_style', { id })
    styles.value = styles.value.filter((s) => s.id !== id)
    // The backend set the defaults that pointed at it back to "No style": follow,
    // so the pickers don't show a deleted style until the next reload.
    for (const p of useProjectsStore().projects) {
      if (p.session_defaults.style === id) p.session_defaults.style = NO_STYLE
    }
    const app = useAppSettingsStore()
    if (!app.settings || app.settings.default_style === id) app.refresh().catch(() => {})
  }

  /** Put back the deleted samples; returns them. */
  async function restoreSamples() {
    const restored = await invoke<ResponseStyle[]>('restore_builtin_styles')
    if (restored.length) {
      styles.value = [...restored, ...styles.value].sort(
        (a, b) => Number(a.project_id !== null) - Number(b.project_id !== null) || a.position - b.position,
      )
    }
    return restored
  }

  return { styles, loaded, load, create, update, remove, restoreSamples }
})
