<script setup lang="ts">
// Small status/label pill. `tone` covers the app's semantic colors; `neutral`
// is the default muted chip used for tags (engine, target, etc.).
import { computed, useAttrs } from 'vue'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

defineOptions({ inheritAttrs: false })

// Tinted tones: a /10–/15 fill with same-hue text, plus a currentColor inset
// ring so the pill keeps its edge on a selected row. The status tokens are
// dark enough to hold 4.5:1 on their own tint now that badges are 12px.
const badge = cva(
  'ui-badge inline-flex items-center gap-1 rounded-full font-medium whitespace-nowrap ring-1 ring-inset ring-current/15',
  {
    variants: {
      tone: {
        neutral: 'bg-secondary text-muted-foreground',
        running: 'bg-info/15 text-info',
        success: 'bg-success/15 text-success',
        error: 'bg-destructive/10 text-destructive',
        warning: 'bg-warning/15 text-warning',
        info: 'bg-violet-500/15 text-violet-700 dark:text-violet-300',
        primary: 'bg-primary/12 text-primary',
      },
      size: {
        xs: 'px-1.5 py-px text-2xs',
        sm: 'px-2 py-0.5 text-2xs',
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
