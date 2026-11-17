<script setup lang="ts">
// One sidebar row: a link to a screen, or (when `action` is set) a button that
// opens a window instead of navigating. 32px high, 14px label, 16px icon; the
// current one gets the accent fill and the 2px primary bar on the left.
// `compact` is the icon rail's version: a 36px icon button, the label (and the
// badge's number) in the tooltip, ↗ on the icon's corner; no badge pill.
import { computed, type Component } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowUpRight } from 'lucide-vue-next'
import SidebarBadge, { type NavBadge } from './SidebarBadge.vue'

const props = defineProps<{
  label: string
  icon: Component
  /** Route to go to (unused for a window button). */
  to?: string
  /** Opens a window; makes the row a button. */
  action?: () => unknown
  active?: boolean
  /** The screen's saved open mode is its own window: shows ↗. */
  inOwnWindow?: boolean
  badge?: NavBadge | null
  /** Icon rail: icon only. */
  compact?: boolean
  /** Compact only: what the badge's number means, appended to the tooltip. */
  badgeTip?: string
}>()

const { t } = useI18n()

const ROW =
  'relative flex h-8 w-full items-center gap-2.5 rounded-md px-3 text-sm transition-colors cursor-pointer select-none ' +
  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'
const IDLE = 'text-muted-foreground hover:text-foreground hover:bg-accent/50'
const RAIL =
  'relative flex h-9 w-full items-center justify-center rounded-md transition-colors cursor-pointer select-none ' +
  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'
const CURRENT = 'is-active bg-accent text-foreground font-medium'

const base = computed(() => (props.compact ? RAIL : ROW))
// A window button says so in its tooltip; in the rail the badge's meaning too.
const tip = computed(() => {
  const parts = [props.label]
  if (props.compact && props.badgeTip) parts.push(props.badgeTip)
  return parts.join(' · ')
})
</script>

<template>
  <button
    v-if="action"
    type="button"
    :class="[base, active ? CURRENT : IDLE]"
    :title="inOwnWindow ? `${tip} — ${t('nav.opensInWindow')}` : tip"
    :aria-label="compact ? tip : undefined"
    @click="action()"
  >
    <component :is="icon" class="h-4 w-4 shrink-0" :stroke-width="1.75" />
    <template v-if="!compact">
      <span class="flex-1 truncate text-left">{{ label }}</span>
      <span v-if="inOwnWindow" class="shrink-0 opacity-70" :title="t('nav.openInWindow')" :aria-label="t('nav.openInWindow')">
        <ArrowUpRight class="h-3.5 w-3.5" :stroke-width="1.75" />
      </span>
      <SidebarBadge :badge="badge" />
    </template>
    <ArrowUpRight v-else-if="inOwnWindow" class="absolute right-1 top-1 h-2.5 w-2.5 opacity-70" :stroke-width="2" />
    <span v-if="inOwnWindow" class="sr-only">({{ t('nav.opensInWindow') }})</span>
  </button>
  <RouterLink
    v-else
    :to="to ?? '/'"
    :class="[base, active ? CURRENT : IDLE]"
    :title="compact ? tip : undefined"
    :aria-label="compact ? tip : undefined"
  >
    <span
      v-if="active"
      class="app-nav-indicator absolute left-0 inset-y-[6px] w-[2px] rounded-r-full bg-primary"
    />
    <component :is="icon" class="h-4 w-4 shrink-0" :stroke-width="1.75" />
    <template v-if="!compact">
      <span class="flex-1 truncate">{{ label }}</span>
      <span v-if="inOwnWindow" class="shrink-0 opacity-70" :title="t('nav.openInWindow')" :aria-label="t('nav.openInWindow')">
        <ArrowUpRight class="h-3.5 w-3.5" :stroke-width="1.75" />
      </span>
      <SidebarBadge :badge="badge" />
    </template>
    <ArrowUpRight v-else-if="inOwnWindow" class="absolute right-1 top-1 h-2.5 w-2.5 opacity-70" :stroke-width="2" />
  </RouterLink>
</template>
