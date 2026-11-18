<script setup lang="ts">
// Heading block at the top of a Settings section: title + one-line intro.
defineProps<{ title: string; intro: string }>()
</script>

<template>
  <div>
    <h3 class="text-lg font-semibold leading-[1.4]">{{ title }}</h3>
    <p class="mt-0.5 max-w-[72ch] text-xs text-muted-foreground">{{ intro }}</p>
  </div>
</template>
