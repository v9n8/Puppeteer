<script setup lang="ts">
// The one-line mark at the end of a turn that was cut short (V3-29). Not a
// message: no buttons, and not part of the ⌘F text (`search-skip`).
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { CircleStop } from 'lucide-vue-next'
import { turnCutLabel, turnCutTooltip, type TurnCut } from '@/lib/turnCut'

const props = defineProps<{ cut: TurnCut }>()
const { t } = useI18n()

const label = computed(() => turnCutLabel(props.cut, new Date(), t))
const tooltip = computed(() => turnCutTooltip(props.cut, t))
</script>

<template>
  <div
    role="note"
    class="turn-cut-mark search-skip my-2 flex min-w-0 items-center gap-1.5 text-[13px] text-muted-foreground"
    :title="tooltip || undefined"
  >
    <CircleStop class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" aria-hidden="true" />
    <span class="min-w-0 break-words">{{ label }}</span>
  </div>
</template>
