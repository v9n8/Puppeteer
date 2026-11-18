<script setup lang="ts">
// Trash toolbar: the search mode (Name · Content), the search box (fills the
// row) with the Search button of the Content mode, and the sort menu.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, ChevronDown, Search } from 'lucide-vue-next'
import { Button, DropdownItem, DropdownMenu, Input, SegmentedControl } from '@/components/ui'
import { TRASH_SORTS, type TrashSearchMode, type TrashSort } from '@/lib/trash'

const props = defineProps<{
  query: string
  sort: TrashSort
  mode: TrashSearchMode
  /** The Content mode is off (its feature is): the button is locked. */
  contentLocked: boolean
  /** A content search is running. */
  searching: boolean
}>()
const emit = defineEmits<{
  'update:query': [value: string]
  'update:sort': [value: TrashSort]
  'update:mode': [value: TrashSearchMode]
  submit: []
}>()

const { t } = useI18n()
const sortLabel = computed(() => t(`trash.sort.${props.sort}`))
const modes: TrashSearchMode[] = ['name', 'content']

const modeOptions = computed(() =>
  modes.map((m) => {
    const locked = m === 'content' && props.contentLocked
    return {
      value: m,
      label: t(`trash.search.mode.${m}`),
      disabled: locked,
      title: locked ? t('trash.content.locked') : undefined,
    }
  }),
)

// Esc clears the text; with nothing typed it keeps its usual meaning.
function onEscape(e: KeyboardEvent) {
  if (!props.query) return
  e.preventDefault()
  e.stopPropagation()
  emit('update:query', '')
}

function onEnter() {
  if (props.mode === 'content' && !props.searching) emit('submit')
}
</script>

<template>
  <div class="flex items-center gap-2 border-b border-border px-6 py-2">
    <SegmentedControl
      radio
      :stretch="false"
      :aria-label="t('trash.search.by')"
      :model-value="mode"
      :options="modeOptions"
      @update:model-value="emit('update:mode', $event)"
    />
    <div class="relative min-w-0 flex-1">
      <Search class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" :stroke-width="2" />
      <Input
        :model-value="query"
        class="w-full pl-8"
        :placeholder="t(mode === 'content' ? 'trash.search.placeholderContent' : 'trash.search.placeholder')"
        :aria-label="t(mode === 'content' ? 'trash.search.placeholderContent' : 'trash.search.placeholder')"
        @update:model-value="emit('update:query', $event)"
        @keydown.esc="onEscape"
        @keydown.enter="onEnter"
      />
    </div>
    <Button
      v-if="mode === 'content'"
      variant="primary"
      size="sm"
      class="min-h-8 shrink-0 max-[901px]:w-8 max-[901px]:px-0"
      :disabled="searching || !query.trim()"
      :title="t('trash.search.submitTitle')"
      :aria-label="searching ? t('trash.search.searching') : t('trash.search.submit')"
      @click="emit('submit')"
    >
      <Search class="h-3.5 w-3.5" :stroke-width="2" />
      <span class="max-[901px]:hidden">{{ searching ? t('trash.search.searching') : t('trash.search.submit') }}</span>
    </Button>
    <DropdownMenu>
      <template #trigger="{ open }">
        <Button variant="outline" size="sm" class="shrink-0 pl-2.5 pr-2" aria-haspopup="menu" :aria-expanded="open">
          {{ t('trash.sortLabel', { sort: sortLabel }) }}
          <ChevronDown class="h-3.5 w-3.5" :stroke-width="2" />
        </Button>
      </template>
      <DropdownItem v-for="value in TRASH_SORTS" :key="value" @click="emit('update:sort', value)">
        <Check class="h-3.5 w-3.5" :class="value === sort ? '' : 'invisible'" :stroke-width="2" />
        {{ t(`trash.sort.${value}`) }}
      </DropdownItem>
    </DropdownMenu>
  </div>
</template>
