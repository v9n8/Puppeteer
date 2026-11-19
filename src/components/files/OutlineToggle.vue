<script setup lang="ts">
// "Contents" button shown above the Markdown while the Contents column is closed (remembered per machine).
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
    size="sm"
    class="ml-3 mt-2 text-muted-foreground"
    :title="t('files.outline.show')"
    @click="ui.setFileOutlineOpen(true)"
  >
    <ListTree class="h-3.5 w-3.5" :stroke-width="1.75" />
    {{ t('files.outline.title') }}
  </Button>
</template>
