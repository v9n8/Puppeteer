<script setup lang="ts">
// First step of "Session with no project open" (sidebar Session → /projects?add=1):
// says why a project is needed, then hands over to the folder picker.
import { useI18n } from 'vue-i18n'
import { FolderOpen } from 'lucide-vue-next'
import { Button, Modal } from '@/components/ui'

defineProps<{ open: boolean; busy: boolean }>()
defineEmits<{ close: []; pick: [] }>()

const { t } = useI18n()
</script>

<template>
  <Modal :open="open" size="sm" :title="t('projects.addProject')" @close="$emit('close')">
    <p class="px-5 py-5 text-sm text-muted-foreground">{{ t('projects.addIntro') }}</p>
    <template #footer>
      <Button variant="outline" size="sm" @click="$emit('close')">{{ t('common.cancel') }}</Button>
      <Button size="sm" :disabled="busy" @click="$emit('pick')">
        <FolderOpen class="h-3.5 w-3.5" :stroke-width="1.75" />
        {{ t('projects.chooseFolder') }}
      </Button>
    </template>
  </Modal>
</template>
