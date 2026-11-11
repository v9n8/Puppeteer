<script setup lang="ts">
// Quick view of a project (UX-52, after AWOG's ProjectQuickViewModal): the
// Projects page's detail column in a large modal, to look at or edit a project
// without leaving the session. Edits go through the projects store, so the
// session screen and the titlebar tabs show them at once. Mounted once in
// App.vue's main window; opened through useProjectQuickView.
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import { X } from 'lucide-vue-next'
import { Button, Modal } from '@/components/ui'
import ProjectDetailPane from '@/components/projects/ProjectDetailPane.vue'
import { useProjectQuickView } from '@/composables/useProjectQuickView'
import type { ProjectPlace } from '@/lib/projectList'
import { useProjectsStore } from '@/stores/projects'

const { t } = useI18n()
const route = useRoute()
const store = useProjectsStore()
const { state, close } = useProjectQuickView()

const project = computed(() => {
  const id = state.value?.projectId
  return id ? store.projects.find((p) => p.id === id) ?? null : null
})
// Removed (from its Overview tab) → closed for good, so restoring it from the
// Trash later doesn't pop the modal back up.
watch(project, (p) => {
  if (!p) close()
})
const place = computed<ProjectPlace>({
  get: () => state.value?.place ?? { tab: 'overview', section: null },
  set: (value) => {
    if (state.value) state.value = { ...state.value, place: value }
  },
})

// A link inside (Manage skills, Settings…) changes the screen underneath; the
// quick view would otherwise stay on top of it.
watch(() => route.path, close)
</script>

<template>
  <Modal :open="!!project" size="2xl" scroll-body hide-header @close="close">
    <div v-if="project" class="flex h-full min-h-0">
      <ProjectDetailPane v-model:place="place" :project="project">
        <template #actions>
          <Button
            variant="ghost"
            size="icon"
            :title="t('common.close')"
            :aria-label="t('common.close')"
            @click="close"
          >
            <X class="h-4 w-4" :stroke-width="1.75" />
          </Button>
        </template>
      </ProjectDetailPane>
    </div>
  </Modal>
</template>
