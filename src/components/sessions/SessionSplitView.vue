<script setup lang="ts">
// The Session area's frame (V3-27): one grid whose parts (tab bar, conversation,
// file area, turn status, composer, info bar) never move in the DOM, only change
// grid cell. So the conversation keeps its scroll and stream whether the Session
// shows one tab at a time (`split` off) or Conversation beside the files.
//
//   tabs:   tab bar / (banner) / conversation or file / status / composer / info
//   split:  [tab bar | divider | file tab bar]
//           [conversation | divider | file area] — status, composer, info under
//           the conversation, the file area running down the right
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Info } from 'lucide-vue-next'
import SplitDivider from './SplitDivider.vue'

const props = defineProps<{
  split: boolean
  /** Left share (0..1), stored. */
  ratio: number
  /** Split is on but the window is too narrow: tabs layout plus a notice. */
  narrowNotice: boolean
  /** Tabs layout: which of conversation / file area is shown (split shows both). */
  showLog: boolean
  showFile: boolean
}>()
const emit = defineEmits<{ commit: [leftPx: number, totalPx: number]; reset: [] }>()

const { t } = useI18n()
const grid = ref<HTMLElement | null>(null)
// The ratio while the divider is held; stored only on release.
const live = ref<number | null>(null)
const ratio = computed(() => live.value ?? props.ratio)
watch(() => props.ratio, () => { live.value = null })
onBeforeUnmount(() => { live.value = null })

const MIN = 'min(360px, 45%)'
const frame = computed(() =>
  props.split
    ? {
        gridTemplateColumns: `minmax(${MIN}, calc((100% - 1px) * ${ratio.value})) 1px minmax(${MIN}, 1fr)`,
        gridTemplateRows: 'auto minmax(0, 1fr) auto auto auto',
        gridTemplateAreas: '"tabbar divider rtabbar" "log divider file" "status divider file" "composer divider file" "info divider file"',
      }
    : {
        gridTemplateColumns: 'minmax(0, 1fr)',
        gridTemplateRows: 'auto auto minmax(0, 1fr) auto auto auto',
        gridTemplateAreas: '"tabbar" "banner" "main" "status" "composer" "info"',
      },
)
const logArea = computed(() => (props.split ? 'log' : 'main'))
const fileArea = computed(() => (props.split ? 'file' : 'main'))

function onCommit(leftPx: number, totalPx: number) {
  live.value = null
  emit('commit', leftPx, totalPx)
}
</script>

<template>
  <div ref="grid" class="grid min-h-0 min-w-0 flex-1 overflow-hidden" :style="frame">
    <div class="flex min-w-0 flex-col" style="grid-area: tabbar"><slot name="tabbar" /></div>

    <div v-if="narrowNotice && !split" class="flex items-start gap-2 border-b border-border bg-muted/40 px-3 py-1.5 text-2xs text-muted-foreground" style="grid-area: banner" role="status">
      <Info class="mt-0.5 h-3.5 w-3.5 shrink-0" :stroke-width="1.75" aria-hidden="true" />
      {{ t('run.splitView.narrowNotice') }}
    </div>

    <template v-if="split">
      <div class="relative z-10 min-h-0" style="grid-area: divider">
        <SplitDivider :ratio="ratio" :container="grid" @drag="live = $event" @commit="onCommit" @reset="emit('reset')" />
      </div>
      <div class="flex min-w-0 flex-col" style="grid-area: rtabbar"><slot name="rtabbar" /></div>
    </template>

    <div v-show="split || showLog" class="flex min-h-0 min-w-0 flex-col overflow-hidden" :style="{ gridArea: logArea }"><slot name="log" /></div>
    <div v-show="split || showFile" class="flex min-h-0 min-w-0 flex-col overflow-hidden" :style="{ gridArea: fileArea }"><slot name="file" /></div>

    <div class="flex min-w-0 flex-col" style="grid-area: status"><slot name="status" /></div>
    <div class="flex min-w-0 flex-col" style="grid-area: composer"><slot name="composer" /></div>
    <div class="flex min-w-0 flex-col" style="grid-area: info"><slot name="info" /></div>
  </div>
</template>
