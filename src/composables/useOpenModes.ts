// Where every detachable screen opens right now (main window / own window),
// following changes made in any window (lib/windowPlacement.ts). The sidebar
// marks screens that live in their own window with it.
import { getCurrentScope, onScopeDispose, readonly, ref } from 'vue'
import {
  loadPlacements,
  onPlacementsChanged,
  openModesOf,
  type DetachableScreen,
} from '@/lib/windowPlacement'

export function useOpenModes() {
  const modes = ref(openModesOf(loadPlacements()))
  const off = onPlacementsChanged((map) => {
    const next = openModesOf(map)
    // Bounds are saved on every window move; only a mode change re-renders.
    const changed = (Object.keys(next) as DetachableScreen[]).some((s) => next[s] !== modes.value[s])
    if (changed) modes.value = next
  })
  if (getCurrentScope()) onScopeDispose(off)

  return {
    modes: readonly(modes),
    isInOwnWindow: (screen: DetachableScreen) => modes.value[screen] === 'window',
  }
}
