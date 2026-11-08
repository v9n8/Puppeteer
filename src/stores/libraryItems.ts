import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/lib/tauri'
import type { LibraryKind, LinkState } from '@/lib/libraryItems'

// The Agents and Commands libraries (backend `library_items`): the same eight
// commands serve both, told apart by `kind`. One store per kind, so the two
// screens keep their own lists.

export interface LibraryItem {
  id: string
  name: string
  description: string
  source_path: string
  updated_at: string
}

export interface LibraryItemContent {
  item: LibraryItem
  content: string
}

export interface AppliedLibraryItem {
  item_id: string
  name: string
  description: string
  applied_at: string
  state: LinkState
}

export interface ItemPayload {
  name: string
  description: string
  content: string
}

function defineItemsStore(kind: LibraryKind) {
  return defineStore(`libraryItems.${kind}`, () => {
    const items = ref<LibraryItem[]>([])
    const loading = ref(false)
    /** Raw backend error of the last list fetch (shown through featureGate's errorText). */
    const error = ref<string | null>(null)
    /** project id → the items applied there, with their link state. */
    const applied = ref<Record<string, AppliedLibraryItem[]>>({})

    async function fetchItems() {
      loading.value = true
      error.value = null
      try {
        items.value = await invoke<LibraryItem[]>('list_library_items', { kind })
      } catch (e) {
        error.value = String(e)
      } finally {
        loading.value = false
      }
    }

    async function getItem(id: string): Promise<LibraryItemContent> {
      return invoke<LibraryItemContent>('get_library_item', { kind, id })
    }

    async function createItem(payload: ItemPayload): Promise<LibraryItem> {
      const item = await invoke<LibraryItem>('create_library_item', { kind, payload })
      await fetchItems()
      return item
    }

    async function updateItem(payload: ItemPayload & { id: string }): Promise<LibraryItem> {
      try {
        return await invoke<LibraryItem>('update_library_item', { kind, payload })
      } finally {
        // "Saved, but syncing to projects failed" is an error after the save: list it anyway.
        await fetchItems()
      }
    }

    async function deleteItem(id: string): Promise<void> {
      await invoke('delete_library_item', { kind, id })
      await fetchItems()
    }

    async function fetchAppliedFor(projectId: string) {
      try {
        const list = await invoke<AppliedLibraryItem[]>('get_applied_library_items', { kind, projectId })
        applied.value = { ...applied.value, [projectId]: list }
      } catch {
        // Unknown for now: the project counts as having none.
        const rest = { ...applied.value }
        delete rest[projectId]
        applied.value = rest
      }
    }

    /** Re-read every project's applied items (one call per project). */
    async function fetchApplied(projectIds: string[]) {
      await Promise.all(projectIds.map(fetchAppliedFor))
      const keep = new Set(projectIds)
      applied.value = Object.fromEntries(Object.entries(applied.value).filter(([id]) => keep.has(id)))
    }

    async function applyToProject(projectId: string, itemId: string) {
      try {
        await invoke('apply_library_item', { kind, projectId, itemId })
      } finally {
        await fetchAppliedFor(projectId)
      }
    }

    async function removeFromProject(projectId: string, itemId: string) {
      try {
        await invoke('remove_library_item_from_project', { kind, projectId, itemId })
      } finally {
        await fetchAppliedFor(projectId)
      }
    }

    return {
      items, loading, error, applied,
      fetchItems, getItem, createItem, updateItem, deleteItem, fetchApplied, applyToProject, removeFromProject,
    }
  })
}

const STORES = { agent: defineItemsStore('agent'), command: defineItemsStore('command') }

export function useLibraryItemsStore(kind: LibraryKind) {
  return STORES[kind]()
}
