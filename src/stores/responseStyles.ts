import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/lib/tauri'
import type { BuiltinStyle, ResponseStyle, ResponseStyleList } from '@/lib/responseStyles'

/**
 * Response styles (UX-17): the built-ins and the user's styles. `styles` holds
 * the global ones plus, after `load(projectId)`, that project's own.
 */
export const useResponseStylesStore = defineStore('responseStyles', () => {
  const builtins = ref<BuiltinStyle[]>([])
  const styles = ref<ResponseStyle[]>([])
  const loaded = ref(false)

  async function load(projectId?: string | null): Promise<ResponseStyleList> {
    const list = await invoke<ResponseStyleList>('list_response_styles', { projectId: projectId ?? null })
    builtins.value = list.builtins
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
  }

  return { builtins, styles, loaded, load, create, update, remove }
})
