<script setup lang="ts">
// Key label ("⌘K"). 12px is the one size below the 13px floor: the v3 design draws
// it that way for a 20px-tall chip, and it does not follow Settings › Font size.
</script>

<template>
  <kbd class="inline-flex h-5 items-center leading-none rounded-[4px] border border-border px-[5px] py-[3px] font-sans text-[12px] font-medium text-muted-foreground">
    <slot />
  </kbd>
</template>
