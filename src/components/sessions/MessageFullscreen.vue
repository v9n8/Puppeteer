<script setup lang="ts">
// Read one chat message — or a whole turn, tool calls included — full screen,
// from the row under each turn (components/sessions/MsgActions.vue). StreamLog
// fills the body. Teleported to body: a log entry can be a `content-visibility`
// container, which would trap a `fixed` overlay inside it.
//
// The body is a selection surface of its own: the bar only covers declared
// areas, and the modal sits outside the log's. Quote and notes go wherever the
// log's surface sends them.
import { ref } from 'vue'
import { Modal } from '@/components/ui'
import { useSelectionSurface, type SelectionSurface } from '@/composables/useSelectionSurface'

const props = defineProps<{
  open: boolean
  title: string
  /** Markdown the body is rendered from (Copy MD). */
  sources: readonly string[]
  /** The log's own surface; null = Translate and Copy MD only. */
  owner: SelectionSurface | null
}>()

const emit = defineEmits<{ close: [] }>()

const bodyEl = ref<HTMLElement | null>(null)

// Read through on every use: the log finds its surface only after it mounts,
// i.e. after this child has been set up.
useSelectionSurface({
  el: bodyEl,
  get allow() {
    return props.owner?.allow ?? { translate: true, copyMd: true }
  },
  markdownSource: () => props.sources,
  quoteTarget: () => props.owner?.quoteTarget?.() ?? null,
  capture: () => props.owner?.capture?.() ?? { context: {} },
})
</script>

<template>
  <Teleport to="body">
    <Modal :open="open" :title="title" size="full" scroll-body @close="emit('close')">
      <div ref="bodyEl" class="px-5 py-4">
        <slot />
      </div>
    </Modal>
  </Teleport>
</template>
