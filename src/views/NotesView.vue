<script setup lang="ts">
// Notes as a screen of the main window (`/notes[?project=][&id=]`), or of its own
// window (views/ItemListWindow.vue): lib/itemListWindow keeps where the user left
// it, DetachButton / MergeBackButton move it. The list and the note are
// components/items/ItemListPane.vue, the same body the window shows. The address
// follows the selected note, so Open in new window carries it along.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { onBeforeRouteLeave, useRoute, useRouter } from 'vue-router'
import { PageHeader } from '@/components/ui'
import DetachButton from '@/components/app/DetachButton.vue'
import ItemListPane from '@/components/items/ItemListPane.vue'
import { notesTarget, openNotesWindow } from '@/lib/itemListWindow'
import { NOTES_PATH, notesPath, parseItemQuery } from '@/lib/screenRoutes'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()

const initial = parseItemQuery(route.query)
const pane = ref<InstanceType<typeof ItemListPane> | null>(null)
const projectFilter = ref(initial.projectId ?? '')
const selectedId = ref<string | null>(initial.id)

// Asked again (sidebar, ⌘K, a linked note) while the screen is open: re-point it.
watch(
  () => route.fullPath,
  () => {
    if (route.path !== NOTES_PATH) return
    const { projectId, id } = parseItemQuery(route.query)
    if (projectId !== null) pane.value?.setScope(projectId)
    if (id && id !== selectedId.value) void pane.value?.selectFresh(id)
  },
)

// The address follows the selection, so a reload or a copied link reopens it.
function syncUrl() {
  if (route.path !== NOTES_PATH) return
  const target = notesPath({ projectId: projectFilter.value, id: selectedId.value })
  if (target === route.fullPath) return
  router.replace(target).catch(() => {})
}
watch([projectFilter, selectedId], syncUrl)
onMounted(syncUrl)

const detachTarget = computed(() => ({
  ...notesTarget,
  openWindow: () => openNotesWindow({ projectId: projectFilter.value, id: selectedId.value }),
}))

/** Leaving with an unsaved edit asks first; true = go ahead. */
async function confirmLeave(): Promise<boolean> {
  if (!pane.value?.hasUnsavedEdit()) return true
  return pane.value.confirmDiscard()
}
// Open in new window asks once, then leaves the screen: the guard must not ask again.
let askedForDetach = false
async function confirmDetach(): Promise<boolean> {
  const ok = await confirmLeave()
  if (ok) {
    askedForDetach = true
    setTimeout(() => (askedForDetach = false), 2000)
  }
  return ok
}
onBeforeRouteLeave(() => askedForDetach || confirmLeave())
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-background">
    <PageHeader :title="t('item.navNotes')">
      <template #actions>
        <DetachButton :target="detachTarget" :confirm-leave="confirmDetach" />
      </template>
    </PageHeader>
    <div class="min-h-0 flex-1">
      <ItemListPane
        ref="pane"
        v-model:project="projectFilter"
        kind="note"
        hide-pin
        :initial-id="initial.id"
        :context-project-id="initial.projectId"
        @selected="selectedId = $event"
      />
    </div>
  </div>
</template>
