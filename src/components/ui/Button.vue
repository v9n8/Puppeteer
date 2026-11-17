<script setup lang="ts">
// Shared button. Centralizes the many ad-hoc primary/outline/ghost button class
// combos scattered across the app so sizing + hover stay consistent everywhere.
// Pass any extra classes via `class` — they're tailwind-merged over the variant.
import { computed, useAttrs } from 'vue'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '@/lib/utils'

defineOptions({ inheritAttrs: false })

const button = cva(
  'inline-flex items-center justify-center whitespace-nowrap rounded-sm border border-transparent font-medium transition cursor-pointer select-none focus:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-50 disabled:pointer-events-none',
  {
    variants: {
      variant: {
        primary: 'bg-primary text-primary-foreground hover:opacity-90 active:scale-[.97]',
        // Hover only swaps the fill; the border stays put.
        outline: 'border border-input bg-card text-foreground hover:bg-accent active:scale-[.97]',
        ghost: 'text-foreground hover:bg-accent active:scale-[.97]',
        // Tinted by default: the warning reads before the click without shouting.
        destructive: 'bg-destructive/10 text-destructive hover:bg-destructive/15',
        // Solid red only where the design draws it (empty the Trash); every other destructive action stays tinted.
        'destructive-solid': 'bg-destructive text-destructive-foreground hover:opacity-90 active:scale-[.97]',
        'destructive-ghost': 'text-muted-foreground/60 hover:text-destructive hover:bg-destructive/10',
        subtle: 'bg-muted text-muted-foreground border border-border/60',
      },
      size: {
        xs: 'h-7 gap-1 px-2.5 text-2xs',
        sm: 'h-8 gap-1.5 px-3 text-xs',
        md: 'h-9 gap-2 px-4 text-sm',
        icon: 'h-7 w-7 font-normal',
        'icon-sm': 'h-7 w-7 font-normal',
        /** Dense spots only (banner retry, chip ×, folder-header actions inside a h-8 bar). */
        'icon-xs': 'h-7 w-7 font-normal',
        'icon-lg': 'h-9 w-9',
      },
    },
    compoundVariants: [
      // The design keeps icon-only ghost buttons muted; only ghost buttons with a label read as --foreground.
      { variant: 'ghost', size: ['icon', 'icon-sm', 'icon-xs', 'icon-lg'], class: 'text-muted-foreground hover:text-foreground' },
    ],
    defaultVariants: { variant: 'primary', size: 'sm' },
  },
)

type ButtonVariants = VariantProps<typeof button>

const props = withDefaults(defineProps<{
  variant?: ButtonVariants['variant']
  size?: ButtonVariants['size']
  type?: 'button' | 'submit' | 'reset'
  disabled?: boolean
}>(), {
  type: 'button',
  disabled: false,
})

const attrs = useAttrs()
const classes = computed(() =>
  cn('ui-btn', `ui-btn-${props.variant ?? 'primary'}`, button({ variant: props.variant, size: props.size }), attrs.class as string | undefined),
)
</script>

<template>
  <button
    v-bind="{ ...attrs, class: undefined }"
    :type="type"
    :disabled="disabled"
    :class="classes"
  >
    <slot />
  </button>
</template>
