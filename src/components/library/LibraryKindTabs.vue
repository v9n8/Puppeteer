<script setup lang="ts">
// Tabs of the Library's kinds with a count each (V3-49): Skills / Rules /
// Commands / Agents, plus Memory on a project. Shared by the Library screen
// and Project › Library › Files. `compact` (a narrow window) draws the same
// choice as a select.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { AppSelect } from '@/components/ui'
import type { LibraryItemKind } from '@/lib/libraryDisk'

const props = defineProps<{
  kinds: readonly LibraryItemKind[]
  counts: Record<string, number>
  compact?: boolean
}>()

const model = defineModel<LibraryItemKind>({ required: true })

const { t } = useI18n()

const options = computed(() =>
  props.kinds.map((k) => ({ value: k, label: `${t(`library.kind.${k}`)} (${props.counts[k] ?? 0})` })),
)

const tabEls = ref<HTMLButtonElement[]>([])

function onKeydown(ev: KeyboardEvent, index: number) {
  const step = ev.key === 'ArrowRight' ? 1 : ev.key === 'ArrowLeft' ? -1 : 0
  if (!step) return
  ev.preventDefault()
  const n = props.kinds.length
  const next = (index + step + n) % n
  model.value = props.kinds[next]
  tabEls.value[next]?.focus()
}
</script>

<template>
  <AppSelect
    v-if="compact"
    :model-value="model"
    :options="options"
    :aria-label="t('library.kindSelect')"
    @update:model-value="(v: string) => (model = v as LibraryItemKind)"
  />
  <div v-else role="tablist" :aria-label="t('library.kindSelect')" class="flex h-12 min-w-0 items-end px-1 overflow-x-auto overflow-y-hidden border-b border-border">
    <button
      v-for="(k, i) in kinds"
      :key="k"
      :ref="(el) => { if (el) tabEls[i] = el as HTMLButtonElement }"
      type="button"
      role="tab"
      :aria-selected="k === model"
      :tabindex="k === model ? 0 : -1"
      class="-mb-px inline-flex h-9 shrink-0 cursor-pointer items-center gap-1 whitespace-nowrap border-b-2 px-2 text-xs font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-primary/50"
      :class="k === model ? 'border-primary text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'"
      @click="model = k"
      @keydown="onKeydown($event, i)"
    >
      {{ t(`library.kind.${k}`) }}
      <span class="tabular-nums text-muted-foreground/80">{{ counts[k] ?? 0 }}</span>
    </button>
  </div>
</template>
