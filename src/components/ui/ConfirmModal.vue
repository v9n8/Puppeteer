<script setup lang="ts">
// Global confirm-dialog host. Mount once (in App.vue); triggered from anywhere
// via useConfirm().confirm(). Replaces the browser-native confirm() so dialogs
// match the app's design system (dark theme, indigo primitives).
import { Modal, Button } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { AlertTriangle } from 'lucide-vue-next'

const { state, respond } = useConfirm()
</script>

<template>
  <Modal :open="state.open" :title="state.title" size="sm" @close="respond(null)">
    <div class="flex gap-3 px-4 py-4">
      <div
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full"
        :class="state.variant === 'destructive' ? 'bg-destructive/10 text-destructive' : 'bg-primary/10 text-primary'"
      >
        <AlertTriangle class="h-4 w-4" :stroke-width="1.75" />
      </div>
      <p class="text-sm text-muted-foreground whitespace-pre-line self-center">{{ state.message }}</p>
    </div>
    <template #footer>
      <!-- Wraps when a second action and a long label don't fit on one line. -->
      <div class="flex w-full flex-wrap justify-end gap-2">
        <Button variant="outline" @click="respond(null)">{{ state.cancelLabel }}</Button>
        <Button v-if="state.altLabel" variant="outline" @click="respond('alt')">{{ state.altLabel }}</Button>
        <Button :variant="state.variant === 'destructive' ? 'destructive' : 'primary'" @click="respond('confirm')">
          {{ state.confirmLabel }}
        </Button>
      </div>
    </template>
  </Modal>
</template>
