<script setup lang="ts">
// Small status/label pill. `tone` covers the app's semantic colors; `neutral`
// is the default muted chip used for tags (engine, target, etc.).
import { computed, useAttrs } from 'vue'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

defineOptions({ inheritAttrs: false })

// Outlined tones: transparent fill, same-hue text and a 1px currentColor border,
// so the chip keeps its edge on a selected row. Neutral keeps the secondary fill
// (as in the mockup) with the --border divider colour.
const badge = cva(
  'ui-badge inline-flex h-[22px] items-center gap-1 rounded-[4px] border border-current bg-transparent text-xs font-medium whitespace-nowrap',
  {
    variants: {
      tone: {
        neutral: 'border-border bg-secondary text-muted-foreground',
        running: 'text-info',
        success: 'text-success',
        error: 'text-destructive',
        warning: 'text-warning',
        info: 'text-violet-700 dark:text-violet-300',
        primary: 'text-primary',
      },
      size: {
        xs: 'px-[7px]',
        sm: 'px-[7px]',
      },
    },
    defaultVariants: { tone: 'neutral', size: 'sm' },
  },
)

type BadgeVariants = VariantProps<typeof badge>

const props = withDefaults(defineProps<{
  tone?: BadgeVariants['tone']
  size?: BadgeVariants['size']
}>(), {})

const attrs = useAttrs()
const classes = computed(() => cn(`ui-badge-${props.tone ?? 'neutral'}`, badge({ tone: props.tone, size: props.size }), attrs.class as string | undefined))
</script>

<template>
  <span v-bind="{ ...attrs, class: undefined }" :class="classes">
    <slot />
  </span>
</template>
