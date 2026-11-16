<script setup lang="ts">
// A note's properties at the top of the Notes detail body: its project. Same
// label-over-field look as TodoMetaFields. (A todo's properties live in
// components/tasks/TaskFieldsGrid.vue.)
import { useI18n } from 'vue-i18n'
import ProjectSelect from '@/components/ProjectSelect.vue'
import type { Note } from '@/stores/notes'

defineProps<{ item: Note }>()

const emit = defineEmits<{ setProject: [projectId: string | null] }>()

const { t } = useI18n()
</script>

<template>
  <div class="shrink-0 border-b border-border/60 px-5 py-3">
    <div class="grid max-w-xl grid-cols-2 gap-2.5">
      <div class="min-w-0 space-y-1">
        <label class="block text-2xs font-medium text-muted-foreground">{{ t('item.fieldProject') }}</label>
        <ProjectSelect
          :model-value="item.project_id"
          variant="outline"
          size="sm"
          @update:model-value="emit('setProject', $event)"
        />
      </div>
    </div>
  </div>
</template>
