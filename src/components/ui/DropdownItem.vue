<script setup lang="ts">
// A row in any menu — the ⋯ DropdownMenu and the right-click ContextMenu share
// it, so an action looks the same wherever the user reaches it from.
import { computed } from 'vue'
import { cn } from '@/lib/utils'

const props = withDefaults(
  defineProps<{
    disabled?: boolean
    /** `destructive` tints the row red (deletes); `primary` highlights the one
     *  row that continues an action already in progress. */
    variant?: 'default' | 'primary' | 'destructive'
  }>(),
  { disabled: false, variant: 'default' },
)

const classes = computed(() =>
  cn(
    'ui-menu-item flex w-full items-center gap-2 whitespace-nowrap min-h-8 rounded-sm px-2.5 py-0 text-xs transition-colors cursor-pointer text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50 disabled:pointer-events-none',
    props.variant === 'destructive'
      ? 'text-destructive hover:bg-destructive/10'
      : props.variant === 'primary'
        ? 'text-primary hover:bg-accent'
        : 'text-popover-foreground hover:bg-accent',
  ),
)
</script>

<template>
  <button type="button" role="menuitem" :disabled="disabled" :class="classes">
    <slot />
  </button>
</template>
