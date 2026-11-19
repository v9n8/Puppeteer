<script setup lang="ts">
// The footer of the "Open windows" overlay while some cards are marked: how many, how
// many cannot move back, and what to do with them all (WindowsOverlay does it).
import { useI18n } from 'vue-i18n'
import { Minus, SquareArrowDownLeft, X } from 'lucide-vue-next'
import { Button } from '@/components/ui'

defineProps<{
  /** Cards marked. */
  count: number
  /** Of them, the ones that can move back to the main window. */
  mergeable: number
}>()
const emit = defineEmits<{
  merge: []
  minimize: []
  close: []
  clear: []
}>()

const { t } = useI18n()
</script>

<template>
  <footer class="flex flex-wrap items-center gap-2 border-t border-border bg-muted px-4 py-2 text-[13px]" role="toolbar" :aria-label="t('windows.sel.count', { count })">
    <span class="font-medium text-foreground tabular-nums">{{ t('windows.sel.count', { count }) }}</span>
    <span v-if="count > mergeable" class="text-muted-foreground">· {{ t('windows.sel.skip', { count: count - mergeable }, count - mergeable) }}</span>
    <span class="flex-1" />
    <Button size="xs" class="gap-1.5 text-[13px]" :disabled="mergeable === 0" @click="emit('merge')">
      <SquareArrowDownLeft class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
      {{ t('windows.sel.merge', { count: mergeable }) }}
    </Button>
    <Button variant="outline" size="xs" class="gap-1.5 text-[13px]" @click="emit('minimize')">
      <Minus class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
      {{ t('windows.minimize') }}
    </Button>
    <Button variant="outline" size="xs" class="gap-1.5 text-[13px]" @click="emit('close')">
      <X class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
      {{ t('windows.sel.close', { count }) }}
    </Button>
    <Button variant="ghost" size="xs" class="text-[13px]" @click="emit('clear')">{{ t('windows.sel.clear') }}</Button>
    <span class="text-muted-foreground"><kbd>Esc</kbd> {{ t('windows.foot.clear') }}</span>
  </footer>
</template>
