<script setup lang="ts">
// Heading block at the top of a Settings section: title + ⓘ holding the intro (UR-34).
import { InfoTip } from '@/components/ui'

defineProps<{ title: string; intro: string }>()
</script>

<template>
  <div class="flex items-center gap-1.5">
    <h3 class="text-lg font-semibold leading-[1.4]">{{ title }}</h3>
    <InfoTip :text="intro" />
  </div>
</template>
