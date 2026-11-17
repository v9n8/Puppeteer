<script setup lang="ts">
// Inline status message (error / warning / success / info): card-colored box
// with a solid tone border, a tone icon, the message, and optional trailing
// actions (retry, dismiss…). Replaces the hand-copied tinted-box blocks.
import { computed, type Component } from 'vue'
import { AlertCircle, AlertTriangle, CheckCircle2, Info, Loader2 } from 'lucide-vue-next'
import { cn } from '@/lib/utils'

const props = withDefaults(defineProps<{
  tone?: 'destructive' | 'warning' | 'success' | 'info'
  /** Override the tone's default icon; `null` hides it. */
  icon?: Component | null
  /** Spinner instead of the icon while something is in progress. */
  loading?: boolean
  /** Announce to screen readers (role alert/status) — for messages that appear
   *  in response to an action, not for static notices. */
  live?: boolean
  class?: string
}>(), {
  tone: 'warning',
  icon: undefined,
  loading: false,
  live: false,
  class: undefined,
})

defineSlots<{
  default?: () => unknown
  actions?: () => unknown
}>()

// Full class strings so Tailwind can see them.
const TONE = {
  destructive: { box: 'border-destructive bg-card', icon: 'text-destructive', glyph: AlertCircle },
  warning: { box: 'border-warning bg-card', icon: 'text-warning', glyph: AlertTriangle },
  success: { box: 'border-success bg-card', icon: 'text-success', glyph: CheckCircle2 },
  info: { box: 'border-info bg-card', icon: 'text-info', glyph: Info },
} as const

const glyph = computed(() =>
  props.loading ? Loader2 : props.icon === undefined ? TONE[props.tone].glyph : props.icon,
)
</script>

<template>
  <div
    :class="cn('flex items-start gap-2 rounded-md border px-3 py-2 text-xs text-foreground', TONE[tone].box, props.class)"
    :role="live ? (tone === 'destructive' ? 'alert' : 'status') : undefined"
  >
    <component :is="glyph" v-if="glyph" class="mt-px h-3.5 w-3.5 shrink-0" :class="[TONE[tone].icon, loading && 'animate-spin']" :stroke-width="2" />
    <div class="min-w-0 flex-1 break-words">
      <slot />
    </div>
    <div v-if="$slots.actions" class="flex shrink-0 items-center gap-1">
      <slot name="actions" />
    </div>
  </div>
</template>
