<script setup lang="ts">
// The content-search result of one Trash session row: the first matching
// snippets with the text highlighted, and how many messages matched. Read-only.
import { useI18n } from 'vue-i18n'
import { snippetParts } from '@/lib/sessionSearch'
import type { TrashContentHit } from '@/lib/trash'

defineProps<{ hit: TrashContentHit; query: string }>()

const { t } = useI18n()
</script>

<template>
  <div class="space-y-1 pt-1">
    <p
      v-for="(snippet, i) in hit.snippets"
      :key="i"
      class="line-clamp-2 break-words text-xs text-foreground/80"
    >
      <template v-for="(part, j) in snippetParts(snippet, query)" :key="j">
        <mark v-if="part.match" class="rounded bg-warning/30 text-inherit">{{ part.text }}</mark>
        <template v-else>{{ part.text }}</template>
      </template>
    </p>
    <p class="text-xs text-muted-foreground">
      {{ t('trash.content.matches', { count: hit.count }, hit.count) }}
      <template v-if="hit.count > hit.snippets.length"> · {{ t('trash.content.firstShown', { count: hit.snippets.length }) }}</template>
    </p>
  </div>
</template>
