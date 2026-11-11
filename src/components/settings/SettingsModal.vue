<script setup lang="ts">
// Settings in a modal over the current screen (E1). Opened through
// useSettingsModal, which the `/settings` route calls; mounted once per window
// in App.vue. The body mounts on open, so each opening reads the settings and
// accounts afresh. A click outside does not close it (E2): only Esc or the X.
import { ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { Modal } from '@/components/ui'
import SettingsView from '@/views/SettingsView.vue'
import { useSettingsModal } from '@/composables/useSettingsModal'

const route = useRoute()
const { open, closeSettings } = useSettingsModal()
const view = ref<InstanceType<typeof SettingsView> | null>(null)

async function requestClose() {
  if (view.value && !(await view.value.mayClose())) return
  closeSettings()
}

// A link inside (Library, a project…) changes the screen underneath; Settings
// would otherwise stay on top of it.
watch(() => route.path, closeSettings)
</script>

<template>
  <Modal :open="open" size="2xl" scroll-body hide-header :dismiss-on-overlay="false" @close="requestClose">
    <SettingsView v-if="open" ref="view" @close="requestClose" />
  </Modal>
</template>
