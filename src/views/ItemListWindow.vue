<script setup lang="ts">
// THE Notes window (opened via lib/itemListWindow, `?itemListWindow=1&kind=note
// [&projectId=][&id=]`) — one window, laid out like the Slack threads window. Only
// the shell lives here: it reads the URL, keeps the OS title (the menu-bar switcher's
// row), the pin, re-pointing when the window is opened again and the unsaved-draft
// check on close. The list and the detail are components/items/ItemListPane.vue,
// which the Tasks screen reuses for todos (a Todos window opened before that
// screen existed loads the Tasks window instead, see AppPopoutHost).
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ConfirmModal, ToastHost } from '@/components/ui'
import ItemListPane from '@/components/items/ItemListPane.vue'
import { useFloatingWindow } from '@/composables/useFloatingWindow'
import type { ItemKind } from '@/lib/itemWindow'
import { itemListSelectEvent, type ItemListSelect } from '@/lib/itemListWindow'
import { refreshTray } from '@/lib/tray'

const { pinned, togglePin } = useFloatingWindow()

const params = new URLSearchParams(window.location.search)
/** The kind this window shows — fixed for its lifetime (one window per kind). */
const kind: ItemKind = params.get('kind') === 'note' ? 'note' : 'todo'
const initialId = params.get('id') || null
const contextProjectId = params.get('projectId') || null

const pane = ref<InstanceType<typeof ItemListPane> | null>(null)
const projectFilter = ref(contextProjectId ?? '')

// The URL follows the selection, so a reload reopens it.
function onSelected(id: string | null) {
  const url = new URL(window.location.href)
  if (id) url.searchParams.set('id', id)
  else url.searchParams.delete('id')
  window.history.replaceState(null, '', url.toString())
}

// The OS window title doubles as this window's row in the menu-bar switcher
// (lib/tray.ts): "Notes: <title>" for the open note.
watch(
  () => pane.value?.windowTitle,
  async (value) => {
    if (!value) return
    document.title = value
    try {
      await getCurrentWindow().setTitle(value)
    } catch {
      /* outside the Tauri shell */
    }
    void refreshTray()
  },
  { immediate: true },
)

let unlisten: UnlistenFn[] = []
/** The close was confirmed: let the next close request through. */
let closing = false

onMounted(async () => {
  try {
    unlisten.push(
      // Reopened from the app: re-point the project scope and / or select an item.
      await listen<ItemListSelect>(itemListSelectEvent(kind), (e) => {
        const p = e.payload
        if (!p) return
        if (p.projectId !== undefined) pane.value?.setScope(p.projectId)
        if (p.id) void pane.value?.selectFresh(p.id)
      }),
    )
    // Closing with an unsaved draft asks first. A listener here makes the close
    // ours: unless prevented, the window is destroyed after it (app quit via
    // app.exit() is not routed through it).
    unlisten.push(
      await getCurrentWindow().onCloseRequested(async (e) => {
        if (closing || !pane.value?.hasUnsavedEdit()) return
        e.preventDefault()
        if (!(await pane.value.confirmDiscard())) return
        closing = true
        await getCurrentWindow().destroy()
      }),
    )
  } catch {
    /* running outside the Tauri shell */
  }
})

onBeforeUnmount(() => {
  unlisten.forEach((off) => off())
  unlisten = []
})
</script>

<template>
  <div class="h-screen w-screen overflow-hidden bg-background text-foreground">
    <ItemListPane
      ref="pane"
      v-model:project="projectFilter"
      :kind="kind"
      :initial-id="initialId"
      :context-project-id="contextProjectId"
      :pinned="pinned"
      @selected="onSelected"
      @toggle-pin="togglePin"
    />
    <!-- The pop-out doesn't mount the main app's dialog/toast hosts. -->
    <ConfirmModal />
    <ToastHost />
  </div>
</template>
