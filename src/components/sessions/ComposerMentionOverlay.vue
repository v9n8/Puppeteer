<script setup lang="ts">
// The `@path` chips of the composer (VF-13, QĐ-F09): a copy of the textarea's
// text laid out right behind it in the same type box, with its letters
// transparent, so only each mention's background and outline show through.
// Typing, selection, undo, IME and scrolling stay the textarea's own; this
// layer takes no pointer and is hidden from assistive tech.
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { splitMentions } from '@/lib/mentionSegments'
import { COMPOSER_TEXT } from './composerTextStyle'

const props = defineProps<{
  /** The composer's text (its v-model value). */
  text: string
  textarea: HTMLTextAreaElement | null
  /** A project file path for a mention token, or null when it names none. */
  resolve: (raw: string) => string | null
}>()

// Vue holds the v-model value back until an IME composition ends, so the text
// is also read straight from the textarea while typing.
const liveText = ref(props.text)
const scrollTop = ref(0)
const width = ref(0)

const segments = computed(() => splitMentions(liveText.value, props.resolve))
const hasMention = computed(() => segments.value.some((s) => s.mention))

// clientWidth leaves out the textarea's scrollbar, so the copy wraps at the
// same width as the text it lies under.
function measure() {
  const el = props.textarea
  if (!el) return
  scrollTop.value = el.scrollTop
  width.value = el.clientWidth
}
function syncFromTextarea() {
  const el = props.textarea
  if (!el) return
  liveText.value = el.value
  measure()
}

watch(() => props.text, (text) => {
  liveText.value = text
  // The composer resizes itself after its text changes; read the box after that.
  nextTick(measure)
})

let observer: ResizeObserver | null = null
watch(() => props.textarea, (el, prev) => {
  if (prev) {
    prev.removeEventListener('input', syncFromTextarea)
    prev.removeEventListener('compositionupdate', syncFromTextarea)
    prev.removeEventListener('scroll', measure)
  }
  observer?.disconnect()
  observer = null
  if (!el) return
  el.addEventListener('input', syncFromTextarea)
  el.addEventListener('compositionupdate', syncFromTextarea)
  el.addEventListener('scroll', measure, { passive: true })
  if (typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver(measure)
    observer.observe(el)
  }
  measure()
}, { immediate: true })

onBeforeUnmount(() => {
  const el = props.textarea
  if (el) {
    el.removeEventListener('input', syncFromTextarea)
    el.removeEventListener('compositionupdate', syncFromTextarea)
    el.removeEventListener('scroll', measure)
  }
  observer?.disconnect()
})
</script>

<template>
  <div
    v-if="hasMention"
    aria-hidden="true"
    data-composer-mention-overlay
    class="pointer-events-none absolute inset-0 z-0 overflow-hidden"
  >
    <div
      :class="[COMPOSER_TEXT, 'whitespace-pre-wrap break-words text-transparent']"
      :style="{ width: `${width}px`, transform: `translateY(${-scrollTop}px)` }"
    ><template v-for="(seg, i) in segments" :key="i"><span
      v-if="seg.mention"
      data-composer-mention
      class="-mx-1 rounded px-1 bg-secondary ring-1 ring-inset ring-border"
    >{{ seg.text }}</span><template v-else>{{ seg.text }}</template></template></div>
  </div>
</template>
