// Quick view of a project (UX-52): the Projects page's detail column in a large
// modal over the current screen, opened from the titlebar tab menu and the
// session status bar's project chip. A single <ProjectQuickView> host (mounted in
// App.vue's main window) reads this shared state.
import { ref } from 'vue'
import type { ProjectPlace } from '@/lib/projectList'

interface QuickViewState {
  projectId: string
  place: ProjectPlace
}

const state = ref<QuickViewState | null>(null)

/** Opens on `place`, by default the Overview tab. */
function open(projectId: string, place: ProjectPlace = { tab: 'overview', section: null }) {
  state.value = { projectId, place }
}

function close() {
  state.value = null
}

export function useProjectQuickView() {
  return { state, open, close }
}
