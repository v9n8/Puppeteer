<script setup lang="ts">
// "+ New" of the MCP screen (sidebar and empty state): a menu with the two ways to add a
// server — fill in the form by hand, or paste setup instructions (UX-87's "Add with AI").
import { useI18n } from 'vue-i18n'
import { ChevronDown, PencilLine, Plus, Sparkles } from 'lucide-vue-next'
import { Button, DropdownItem, DropdownMenu } from '@/components/ui'

withDefaults(defineProps<{ align?: 'left' | 'right' }>(), { align: 'right' })

const emit = defineEmits<{
  manual: []
  ai: []
}>()

const { t } = useI18n()
</script>

<template>
  <DropdownMenu :align="align">
    <template #trigger>
      <Button size="sm" class="shrink-0">
        <Plus class="h-3.5 w-3.5" :stroke-width="2" />
        {{ t('common.new') }}
        <ChevronDown class="h-3 w-3 opacity-70" :stroke-width="2" />
      </Button>
    </template>
    <DropdownItem @click="emit('manual')">
      <PencilLine class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      <span class="flex flex-col">
        <span>{{ t('mcp.list.newManual') }}</span>
        <span class="text-2xs text-muted-foreground">{{ t('mcp.list.newManualHint') }}</span>
      </span>
    </DropdownItem>
    <DropdownItem @click="emit('ai')">
      <Sparkles class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      <span class="flex flex-col">
        <span>{{ t('mcp.aiAdd.title') }}</span>
        <span class="text-2xs text-muted-foreground">{{ t('mcp.list.newAiHint') }}</span>
      </span>
    </DropdownItem>
  </DropdownMenu>
</template>
