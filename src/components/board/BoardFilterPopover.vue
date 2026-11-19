<script setup lang="ts">
// "Filter" in the board header (UR-61): which columns the board shows. "No flag"
// first, then one group per source (shared, each Project) with its flags indented
// under a pill label. Ticks are a draft until "Apply"; "Undo changes" puts the draft
// back. Clicking outside or Esc with a changed draft asks first (Cancel keeps the
// popover open · Discard · Apply). The rules live in lib/boardFilter.
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { onClickOutside } from '@vueuse/core'
import { Check, CircleDashed, ListFilter, Undo2 } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { usePopoverFocus } from '@/composables/usePopoverFocus'
import { flagLabel, type SessionFlag } from '@/lib/sessionFlags'
import { projectColor, swatchClass } from '@/lib/projectAppearance'
import { NO_FLAG_COLUMN } from '@/lib/flagBoard'
import { sameFilter, toggleColumn, SHARED_GROUP, type BoardFilter } from '@/lib/boardFilter'

const props = defineProps<{
  /** The filter the board shows now. */
  filter: BoardFilter
  flags: SessionFlag[]
  projects: { id: string; name: string; color: string | null }[]
  /** Columns on, shown on the button; null when the filter is the default. */
  badge: number | null
}>()

const emit = defineEmits<{ apply: [filter: BoardFilter] }>()
const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
const { choose } = useConfirm()

const clone = (f: BoardFilter): BoardFilter => JSON.parse(JSON.stringify(f))
const draft = ref<BoardFilter>(clone(props.filter))
watch(open, (isOpen) => { if (isOpen) draft.value = clone(props.filter) })
const dirty = computed(() => !sameFilter(draft.value, props.filter))
const isOn = (id: string) => draft.value.on.includes(id)

const flagMap = computed(() => new Map(props.flags.map((f) => [f.id, f])))
const groups = computed(() =>
  draft.value.groups
    .map((id) => {
      const project = props.projects.find((p) => p.id === id)
      return {
        id,
        name: id === SHARED_GROUP ? t('board.filter.shared') : (project?.name ?? id),
        dot: id === SHARED_GROUP ? 'bg-muted-foreground' : swatchClass(projectColor(project?.color) ?? 'primary'),
        flags: (draft.value.flags[id] ?? []).map((f) => flagMap.value.get(f)).filter((f): f is SessionFlag => !!f),
      }
    })
    .filter((g) => g.flags.length),
)

function toggle(id: string) {
  draft.value = toggleColumn(draft.value, id)
}

function revert() {
  draft.value = clone(props.filter)
}

function apply() {
  emit('apply', clone(draft.value))
  open.value = false
}

// While the confirm box is up, clicks and Esc belong to it.
const confirming = ref(false)

async function requestClose() {
  if (confirming.value) return
  if (!dirty.value) {
    open.value = false
    return
  }
  confirming.value = true
  const choice = await choose({
    title: t('board.filter.confirmTitle'),
    message: t('board.filter.confirmBody'),
    altLabel: t('board.filter.discard'),
    confirmLabel: t('board.filter.apply'),
    variant: 'primary',
  })
  confirming.value = false
  if (choice === 'confirm') apply()
  else if (choice === 'alt') open.value = false
}

function toggleOpen() {
  if (open.value) void requestClose()
  else open.value = true
}

const wrap = ref<HTMLElement | null>(null)
const panel = ref<HTMLElement | null>(null)
onClickOutside(wrap, () => { if (open.value) void requestClose() })
usePopoverFocus(open, wrap, panel)

// Esc in the capture phase, as useEscapeClose, but it asks first; the confirm box handles its own Esc.
function onKey(e: KeyboardEvent) {
  if (e.key !== 'Escape' || confirming.value) return
  e.stopPropagation()
  void requestClose()
}
watch(open, (v) => {
  if (v) window.addEventListener('keydown', onKey, true)
  else window.removeEventListener('keydown', onKey, true)
})
onBeforeUnmount(() => window.removeEventListener('keydown', onKey, true))

const ROW = 'flex h-8 cursor-pointer items-center gap-2 rounded pr-2 hover:bg-accent has-[input:focus-visible]:ring-2 has-[input:focus-visible]:ring-ring'
</script>

<template>
  <div ref="wrap" class="relative shrink-0">
    <button
      type="button"
      class="inline-flex h-8 shrink-0 cursor-pointer items-center gap-1.5 rounded-md border border-input px-2.5 text-sm text-foreground transition-colors hover:bg-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :class="open ? 'bg-accent' : 'bg-card'"
      :title="t('board.filter.tooltip')"
      aria-haspopup="dialog"
      :aria-expanded="open"
      @click="toggleOpen"
    >
      <ListFilter class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
      {{ t('board.filter.button') }}
      <span v-if="badge !== null" class="rounded-full bg-primary/12 px-1.5 text-2xs font-medium text-primary">{{ badge }}</span>
    </button>

    <div
      v-if="open"
      ref="panel"
      role="dialog"
      :aria-label="t('board.filter.button')"
      class="absolute left-0 top-full z-50 mt-1.5 w-[280px] max-w-[calc(100vw-2rem)] overflow-hidden rounded-md border border-border bg-popover text-sm shadow-lg shadow-black/20"
    >
      <div class="relative max-h-[420px] overflow-y-auto px-1 pb-1">
        <ul class="pt-1">
          <li>
            <label :class="ROW" class="pl-0.5">
              <span class="w-3.5 shrink-0" aria-hidden="true" />
              <input type="checkbox" class="sr-only" :checked="isOn(NO_FLAG_COLUMN)" @change="toggle(NO_FLAG_COLUMN)" />
              <span v-if="isOn(NO_FLAG_COLUMN)" class="flex h-4 w-4 shrink-0 items-center justify-center rounded-[4px] border border-primary bg-primary text-primary-foreground"><Check class="h-3 w-3" :stroke-width="3" /></span>
              <span v-else class="h-4 w-4 shrink-0 rounded-[4px] border border-input bg-card" />
              <CircleDashed class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
              <span class="min-w-0 flex-1 truncate">{{ t('board.column.none') }}</span>
            </label>
          </li>
        </ul>
        <section v-for="(g, i) in groups" :key="g.id" :data-filter-group="g.id">
          <div v-if="i" class="my-1 h-px bg-border" />
          <div class="flex items-center gap-1 pb-1 pl-0.5 pt-2">
            <span class="inline-flex h-6 min-w-0 items-center gap-1.5 rounded-full bg-muted px-2.5 text-2xs text-muted-foreground">
              <span class="h-2 w-2 shrink-0 rounded-full" :class="g.dot" aria-hidden="true" />
              <span class="truncate">{{ g.name }}</span>
            </span>
          </div>
          <ul>
            <li v-for="f in g.flags" :key="f.id">
              <label :class="ROW" class="pl-5">
                <span class="w-3.5 shrink-0" aria-hidden="true" />
                <input type="checkbox" class="sr-only" :checked="isOn(f.id)" @change="toggle(f.id)" />
                <span v-if="isOn(f.id)" class="flex h-4 w-4 shrink-0 items-center justify-center rounded-[4px] border border-primary bg-primary text-primary-foreground"><Check class="h-3 w-3" :stroke-width="3" /></span>
                <span v-else class="h-4 w-4 shrink-0 rounded-[4px] border border-input bg-card" />
                <span class="h-2.5 w-2.5 shrink-0 rounded-full" :class="swatchClass(f.color)" aria-hidden="true" />
                <span class="min-w-0 flex-1 truncate">{{ flagLabel(f, t) }}</span>
              </label>
            </li>
          </ul>
        </section>
      </div>
      <div class="flex items-center justify-between gap-2 border-t border-border p-1.5">
        <Button variant="ghost" size="xs" class="px-2 text-sm font-normal" :disabled="!dirty" @click="revert">
          <Undo2 class="h-3.5 w-3.5" :stroke-width="1.75" />{{ t('board.filter.revert') }}
        </Button>
        <Button size="xs" class="px-3 text-sm" :disabled="!dirty" @click="apply">{{ t('board.filter.apply') }}</Button>
      </div>
    </div>
  </div>
</template>
