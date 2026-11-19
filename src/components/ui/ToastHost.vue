<script setup lang="ts">
// App-wide toast stack. Mount once in App.vue; content is driven by useToast.
import { useToast, type ToastAction, type ToastVariant } from '@/composables/useToast'
import { useI18n } from 'vue-i18n'
import { CheckCircle2, XCircle, Info, X } from 'lucide-vue-next'

const { t: tr } = useI18n()
const { state, dismiss } = useToast()

function runAction(id: number, action: ToastAction) {
  dismiss(id)
  action.run()
}

const ICONS = { success: CheckCircle2, error: XCircle, info: Info }
const TONES: Record<ToastVariant, string> = {
  success: 'text-emerald-500',
  error: 'text-destructive',
  info: 'text-primary',
}
</script>

<template>
  <Teleport to="body">
    <div class="fixed bottom-4 right-4 z-[100] flex flex-col gap-2 pointer-events-none">
      <TransitionGroup
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="opacity-0 translate-y-2"
        enter-to-class="opacity-100 translate-y-0"
        leave-active-class="transition duration-150 ease-in absolute right-0"
        leave-from-class="opacity-100"
        leave-to-class="opacity-0 translate-y-1"
      >
        <div
          v-for="t in state.items"
          :key="t.id"
          class="ui-toast pointer-events-auto flex items-center gap-2 min-w-56 max-w-sm bg-card border border-border rounded-lg shadow-md px-3.5 py-2.5 text-xs cursor-pointer"
          role="status"
          @click="dismiss(t.id)"
        >
          <component
            :is="ICONS[t.variant]"
            class="h-4 w-4 shrink-0"
            :class="TONES[t.variant]"
            :stroke-width="2"
          />
          <span class="min-w-0 flex-1 text-foreground leading-snug [overflow-wrap:anywhere]">{{ t.message }}</span>
          <button
            v-if="t.action"
            type="button"
            class="inline-flex min-h-7 shrink-0 items-center rounded-md border border-border px-2 py-1 font-medium text-primary hover:bg-accent"
            @click.stop="runAction(t.id, t.action)"
          >
            {{ t.action.label }}
          </button>
          <button
            v-if="t.action"
            type="button"
            class="shrink-0 rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :aria-label="tr('common.close')"
            :title="tr('common.close')"
            @click.stop="dismiss(t.id)"
          >
            <X class="h-3.5 w-3.5" :stroke-width="2" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>
