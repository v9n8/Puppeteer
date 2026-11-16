// The project / run on screen, for filing a new todo / note (or opening a list
// window) under it. Shared by the app menu (App.vue) and the sidebar buttons.
import { useRoute } from 'vue-router'

export function useRouteCaptureContext() {
  const route = useRoute()

  // File the capture under whatever the user is looking at, so capturing from a
  // run needs no project picking at all.
  function routeCaptureContext() {
    return {
      projectId: typeof route.params.projectId === 'string' ? route.params.projectId : null,
      runId: typeof route.params.runId === 'string' ? route.params.runId : null,
    }
  }

  return routeCaptureContext
}
