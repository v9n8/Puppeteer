<script setup lang="ts">
// Toolbar button that shows / hides the Markdown Contents column (remembered per machine).
import { useI18n } from 'vue-i18n'
import { ListTree } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { useUILayoutStore } from '@/stores/uiLayout'

const { t } = useI18n()
const ui = useUILayoutStore()
</script>

<template>
  <Button
    variant="ghost"
    size="icon-sm"
    class="shrink-0"
    :class="{ 'bg-primary/15 text-primary': ui.fileOutlineOpen }"
    :title="ui.fileOutlineOpen ? t('files.outline.hide') : t('files.outline.show')"
    :aria-label="t('files.outline.title')"
    :aria-pressed="ui.fileOutlineOpen"
    @click="ui.setFileOutlineOpen(!ui.fileOutlineOpen)"
  >
    <ListTree class="h-3.5 w-3.5" :stroke-width="1.75" />
  </Button>
</template>
