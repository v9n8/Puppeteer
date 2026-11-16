<script setup lang="ts">
// Trash toolbar: the name search (fills the row) and the sort menu.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, ChevronDown, Search } from 'lucide-vue-next'
import { Button, DropdownItem, DropdownMenu, Input } from '@/components/ui'
import { TRASH_SORTS, type TrashSort } from '@/lib/trash'

const props = defineProps<{ query: string; sort: TrashSort }>()
const emit = defineEmits<{ 'update:query': [value: string]; 'update:sort': [value: TrashSort] }>()

const { t } = useI18n()
const sortLabel = computed(() => t(`trash.sort.${props.sort}`))

// Esc clears the text; with nothing typed it keeps its usual meaning.
function onEscape(e: KeyboardEvent) {
  if (!props.query) return
  e.preventDefault()
  e.stopPropagation()
  emit('update:query', '')
}
</script>

<template>
  <div class="flex items-center gap-2 border-b border-border px-6 py-2">
    <div class="relative min-w-0 flex-1">
      <Search class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" :stroke-width="2" />
      <Input
        :model-value="query"
        class="w-full pl-8"
        :placeholder="t('trash.search.placeholder')"
        :aria-label="t('trash.search.placeholder')"
        @update:model-value="emit('update:query', $event)"
        @keydown.esc="onEscape"
      />
    </div>
    <DropdownMenu>
      <template #trigger="{ open }">
        <Button variant="outline" size="sm" class="shrink-0" aria-haspopup="menu" :aria-expanded="open">
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
