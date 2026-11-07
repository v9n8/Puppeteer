import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import {
  CMD_GET_FEATURES,
  EVT_FEATURES_CHANGED,
  isFeatureOn,
  type FeatureTree,
} from '@/lib/featureGate'

/**
 * The resolved feature tree of this Puppeteer — or, in a remote-desktop window,
 * of the controlled one (the IPC shim routes both the command and the event).
 * Every window starts it (App.vue) and keeps it current from `features:changed`,
 * so a switch flipped in Settings reaches pop-outs without a reload.
 *
 * Until `loaded`, `isOn` is false for everything: callers that would otherwise
 * show, then hide, an entry wait for `loaded` before rendering it.
 */
export const useFeaturesStore = defineStore('features', () => {
  const tree = ref<FeatureTree>([])
  const loaded = ref(false)
  const error = ref<string | null>(null)

  let starting: Promise<void> | null = null
  // Bumped by every event, so a fetch that was in flight when a newer tree
  // arrived does not overwrite it.
  let version = 0

  async function refresh(): Promise<void> {
    const seen = version
    try {
      const next = await invoke<FeatureTree>(CMD_GET_FEATURES)
      if (seen === version) tree.value = next
      error.value = null
    } catch (e) {
      error.value = String(e)
    } finally {
      // Even on failure: an empty tree hides gated entries instead of leaving
      // the sidebar waiting forever.
      loaded.value = true
    }
  }

  /**
   * Subscribe to `features:changed`, then fetch the tree. Idempotent; the
   * subscription lives as long as the window.
   */
  function start(): Promise<void> {
    if (!starting) {
      starting = (async () => {
        try {
          await listen<FeatureTree>(EVT_FEATURES_CHANGED, (e) => {
            if (!e.payload) return
            version++
            tree.value = e.payload
            loaded.value = true
          })
        } catch {
          // Outside the Tauri shell (browser dev).
        }
        await refresh()
      })()
    }
    return starting
  }

  /** Module (`productivity`) or prop (`productivity.focus`) id. */
  function isOn(id: string): boolean {
    return isFeatureOn(tree.value, id)
  }

  /**
   * `isOn` once the first load is done — for entries reached before the
   * sidebar renders (a cold-start deep link, a backend event).
   */
  async function isOnAfterLoad(id: string): Promise<boolean> {
    await start()
    return isOn(id)
  }

  return { tree, loaded, error, start, refresh, isOn, isOnAfterLoad }
})
