<script setup lang="ts">
// The images attached to a sent prompt (RT-24): thumbnails in the bubble, and a
// full-window viewer on click, after SlackImageLightbox: Esc or a click on the
// backdrop closes it, ‹ / › (and the arrow keys) step through the images.
import { onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronLeft, ChevronRight, X } from 'lucide-vue-next'
import type { ImageAttachment } from '@/lib/streamEvents'
import { imageDataUrl, stepImageIndex } from '@/lib/promptImages'

const props = defineProps<{ images: ImageAttachment[] }>()

const { t } = useI18n()

/** The image on screen, or null while the viewer is shut. */
const shown = ref<number | null>(null)

function open(i: number) {
  shown.value = i
}

function close() {
  shown.value = null
}

function step(delta: number) {
  if (shown.value === null || props.images.length < 2) return
  shown.value = stepImageIndex(shown.value, delta, props.images.length)
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    close()
  } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    e.preventDefault()
    e.stopPropagation()
    step(e.key === 'ArrowLeft' ? -1 : 1)
  }
}

// Capture phase, only while open: the window's own shortcuts must not see these keys.
watch(shown, (now, before) => {
  if (now !== null && before === null) window.addEventListener('keydown', onKey, true)
  else if (now === null && before !== null) window.removeEventListener('keydown', onKey, true)
})
// The entry may go away (paging, switching session) with the viewer open.
watch(() => props.images.length, (n) => {
  if (shown.value !== null && shown.value >= n) close()
})
onBeforeUnmount(() => window.removeEventListener('keydown', onKey, true))
</script>

<template>
  <div class="flex flex-wrap gap-1.5">
    <button
      v-for="(img, ii) in images"
      :key="ii"
      type="button"
      class="cursor-zoom-in rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :title="t('streamLog.promptImages.open')"
      :aria-label="t('streamLog.promptImages.open')"
      @click="open(ii)"
    >
      <img
        :src="imageDataUrl(img)"
        class="max-h-40 max-w-48 rounded-md border border-primary/20 object-contain"
        :alt="t('streamLog.promptImages.alt')"
      />
    </button>

    <!-- Teleported: a transformed ancestor in the log would trap position: fixed. -->
    <Teleport to="body">
      <div
        v-if="shown !== null && images[shown]"
        class="fixed inset-0 z-50 flex flex-col bg-black/85"
        role="dialog"
        aria-modal="true"
        @click.self="close"
      >
        <div class="flex shrink-0 items-center gap-2 px-4 py-2 text-white/90">
          <span class="min-w-0 flex-1 truncate text-sm">
            {{ t('streamLog.promptImages.alt') }}
            <span v-if="images.length > 1" class="ml-2 text-xs text-white/60">
              {{ t('streamLog.promptImages.count', { index: shown + 1, total: images.length }) }}
            </span>
          </span>
          <button
            type="button"
            class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-md text-white/80 hover:bg-white/10 hover:text-white"
            :title="t('streamLog.promptImages.close')"
            :aria-label="t('streamLog.promptImages.close')"
            @click="close"
          >
            <X class="h-4 w-4" :stroke-width="2" />
          </button>
        </div>

        <div class="relative flex min-h-0 flex-1 items-center justify-center px-14 pb-6" @click.self="close">
          <img
            :src="imageDataUrl(images[shown])"
            :alt="t('streamLog.promptImages.alt')"
            class="max-h-full max-w-full object-contain"
          />
          <template v-if="images.length > 1">
            <button
              type="button"
              class="absolute left-3 top-1/2 flex h-10 w-10 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full bg-white/10 text-white hover:bg-white/20"
              :title="t('streamLog.promptImages.prev')"
              :aria-label="t('streamLog.promptImages.prev')"
              @click="step(-1)"
            >
              <ChevronLeft class="h-5 w-5" :stroke-width="2" />
            </button>
            <button
              type="button"
              class="absolute right-3 top-1/2 flex h-10 w-10 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full bg-white/10 text-white hover:bg-white/20"
              :title="t('streamLog.promptImages.next')"
              :aria-label="t('streamLog.promptImages.next')"
              @click="step(1)"
            >
              <ChevronRight class="h-5 w-5" :stroke-width="2" />
            </button>
          </template>
        </div>
      </div>
    </Teleport>
  </div>
</template>
