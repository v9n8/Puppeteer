<script setup lang="ts">
// Select of the Library's kinds with a count each (V3-49, UR-30): Skills /
// Rules / Commands / Agents, then Hooks / Plugins on the Library screen (LH),
// plus Memory on a project. Shared by the Library screen and Project › Library
// › Files (files only). A select (not tabs) so all kinds stay
// reachable in a narrow column.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { AppSelect } from '@/components/ui'
import type { LibraryItemKind } from '@/lib/libraryDisk'

const props = defineProps<{
  kinds: readonly LibraryItemKind[]
  counts: Record<string, number>
}>()

const model = defineModel<LibraryItemKind>({ required: true })

const { t } = useI18n()

const options = computed(() =>
  props.kinds.map((k) => ({ value: k, label: `${t(`library.kind.${k}`)} (${props.counts[k] ?? 0})` })),
)
</script>

<template>
  <AppSelect
    :model-value="model"
    :options="options"
    :aria-label="t('library.kindSelect')"
    @update:model-value="(v: string) => (model = v as LibraryItemKind)"
  />
</template>
