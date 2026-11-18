<script setup lang="ts">
// The one count pill: page header, tab strips and sidebar rows share this
// shape. `default` is the neutral secondary chip; `warning` / `danger` are
// solid tone fills (the border takes the fill colour so the size stays equal).
withDefaults(defineProps<{ tone?: 'default' | 'warning' | 'danger' }>(), { tone: 'default' })
</script>

<template>
  <span
    class="flex h-5 min-w-5 shrink-0 items-center justify-center rounded-full border px-[5px] text-2xs font-semibold leading-none"
    :class="{
      'border-border bg-secondary text-foreground': tone === 'default',
      'border-warning bg-warning text-warning-foreground': tone === 'warning',
      'border-destructive bg-destructive text-destructive-foreground': tone === 'danger',
    }"
  >
    <slot />
  </span>
</template>
