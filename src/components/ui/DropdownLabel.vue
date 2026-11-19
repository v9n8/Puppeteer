<script setup lang="ts">
// A group heading inside a menu or popover. Sentence case on purpose: upper-case
// tracked headings read as shouting next to the items and break for non-Latin
// text. The host adds its own padding / margin through `class`.
</script>

<template>
  <div class="text-2xs font-medium text-muted-foreground select-none">
    <slot />
  </div>
</template>
