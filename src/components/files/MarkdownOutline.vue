<script setup lang="ts">
// The "Contents" column beside a rendered Markdown file: its headings (levels 1–3),
// the one being read highlighted, a click scrolling to it.
//
// The headings come from the source (lib/markdownOutline) but the scrolling needs
// the rendered elements, and the renderer gives them no id. So each outline entry
// is matched, in order, to a heading of the rendered `previewEl` by its text, and
// that element gets the entry's slug as id. The match is redone whenever the
// preview's markup changes (new content, markdown renderer finishing its load).
import { computed, onBeforeUnmount, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { X } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { markdownOutline, type OutlineItem } from '@/lib/markdownOutline'
import { useUILayoutStore } from '@/stores/uiLayout'

const props = defineProps<{
  source: string
  /** The element holding the rendered markdown. */
  previewEl: HTMLElement | null
  /** The scrolling ancestor of the preview. */
  scrollEl: HTMLElement | null
}>()

const { t } = useI18n()
const ui = useUILayoutStore()

interface Found { item: OutlineItem; el: HTMLElement }
const found = shallowRef<Found[]>([])
const active = shallowRef('')
const all = computed(() => markdownOutline(props.source))

function norm(s: string | null): string {
  return (s ?? '').toLowerCase().replace(/[^\p{L}\p{N}]/gu, '')
}

function updateActive() {
  const list = found.value
  if (!props.scrollEl || !list.length) { active.value = ''; return }
  const line = props.scrollEl.getBoundingClientRect().top + 24
  let cur = list[0].item.slug
  for (const f of list) {
    if (f.el.getBoundingClientRect().top <= line) cur = f.item.slug
    else break
  }
  active.value = cur
}

function scan() {
  const root = props.previewEl
  if (!root) { found.value = []; return }
  const dom = Array.from(root.querySelectorAll<HTMLElement>('h1,h2,h3,h4,h5,h6'))
  const out: Found[] = []
  let cursor = 0
  for (const item of all.value) {
    for (let i = cursor; i < dom.length; i++) {
      if (norm(dom[i].textContent) !== norm(item.text)) continue
      dom[i].id = `fv-${item.slug}`
      if (item.level <= 3) out.push({ item, el: dom[i] })
      cursor = i + 1
      break
    }
  }
  found.value = out
  updateActive()
}

let observer: MutationObserver | null = null
watch(
  () => props.previewEl,
  (el) => {
    observer?.disconnect()
    observer = null
    if (el) {
      observer = new MutationObserver(scan)
      observer.observe(el, { childList: true, subtree: true })
    }
    scan()
  },
  { immediate: true },
)
watch(all, scan)

let scrolled: HTMLElement | null = null
const onScroll = () => updateActive()
watch(
  () => props.scrollEl,
  (el) => {
    scrolled?.removeEventListener('scroll', onScroll)
    scrolled = el
    el?.addEventListener('scroll', onScroll, { passive: true })
    updateActive()
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  observer?.disconnect()
  scrolled?.removeEventListener('scroll', onScroll)
})

function go(f: Found) {
  active.value = f.item.slug
  f.el.scrollIntoView({ block: 'start', behavior: 'smooth' })
}
</script>

<template>
  <nav class="md-outline sticky top-0 shrink-0 flex flex-col gap-0.5 px-2 py-3 border-r border-border overflow-y-auto" :aria-label="t('files.outline.title')">
    <div class="mx-2 mb-1 flex items-center justify-between gap-1">
      <p class="min-w-0 truncate text-[13px] font-semibold text-foreground">{{ t('files.outline.title') }}</p>
      <Button variant="ghost" size="icon-sm" class="shrink-0" :title="t('files.outline.hide')" :aria-label="t('files.outline.hide')" @click="ui.setFileOutlineOpen(false)">
        <X class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
    </div>
    <button
      v-for="f in found"
      :key="f.item.slug"
      type="button"
      class="flex items-center min-h-[28px] rounded-md px-2 text-left text-[13px] cursor-pointer hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
      :class="active === f.item.slug ? 'bg-accent text-foreground' : 'text-muted-foreground'"
      :style="{ paddingLeft: `${8 + (f.item.level - 1) * 12}px` }"
      :aria-current="active === f.item.slug ? 'location' : undefined"
      :title="f.item.text"
      @click="go(f)"
    >
      <span class="truncate">{{ f.item.text }}</span>
    </button>
  </nav>
</template>

<style scoped>
.md-outline {
  width: 200px;
  max-height: calc(100vh - 8rem);
}
@media (max-width: 900px) {
  .md-outline {
    width: 160px;
  }
}
</style>
