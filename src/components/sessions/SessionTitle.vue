<script setup lang="ts">
// The Session's name in its header: click to rename in place (✓ / Enter saves,
// ✕ / Esc cancels). A session that has not sent a prompt yet only reads
// "New session". Saving goes through the History list's applyRename, so the
// toast and the History row stay the same as renaming there.
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, X } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { useRunSession } from './useRunSession'
import { titleEditRequest } from './titleEditRequest'

const { t } = useI18n()
const { currentRun, runLabel, applyRename } = useRunSession()

const isNew = computed(() => {
  const run = currentRun.value
  return !!run && run.run_type === 'session' && run.status === 'fetched' && !run.started_at
})
const label = computed(() => {
  if (!currentRun.value) return t('run.noRunSelected')
  return isNew.value ? t('run.newSession') : runLabel(currentRun.value)
})
const canRename = computed(() => !!currentRun.value && !isNew.value)

const editing = ref(false)
const draft = ref('')
const inputEl = ref<HTMLInputElement | null>(null)

function start() {
  if (!canRename.value || !currentRun.value) return
  draft.value = currentRun.value.title ?? runLabel(currentRun.value)
  editing.value = true
  nextTick(() => {
    inputEl.value?.focus()
    inputEl.value?.select()
  })
}
// The ⋯ menu's Rename (session window, no History row) asks for the same field.
watch(titleEditRequest, start)
function cancel() {
  editing.value = false
}
// The header stays mounted when another Session of the same Project opens, so an
// edit left open must not carry its draft over to that Session.
watch(() => currentRun.value?.id, cancel)
async function save() {
  const run = currentRun.value
  if (!editing.value || !run) return
  editing.value = false
  await applyRename(run.id, draft.value)
}
</script>

<template>
  <form v-if="editing" class="flex min-w-0 flex-1 items-center gap-1" @submit.prevent="save">
    <input
      ref="inputEl"
      v-model="draft"
      type="text"
      class="h-7 min-w-0 flex-1 rounded border border-border bg-background px-2 text-sm font-semibold text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :aria-label="t('run.rename')"
      @keydown.esc.prevent="cancel"
    />
    <Button
      type="submit"
      variant="ghost"
      size="icon-xs"
      class="shrink-0"
      :title="t('run.sessionTitle.save')"
      :aria-label="t('run.sessionTitle.save')"
    >
      <Check class="h-3.5 w-3.5 text-primary" :stroke-width="2" />
    </Button>
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      class="shrink-0"
      :title="t('run.sessionTitle.cancel')"
      :aria-label="t('run.sessionTitle.cancel')"
      @click="cancel"
    >
      <X class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
  </form>
  <h1 v-else class="min-w-0 text-sm font-semibold">
    <button
      v-if="canRename"
      type="button"
      class="block max-w-full truncate rounded px-1 py-1 text-left font-semibold transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring cursor-text"
      :title="`${label} · ${t('run.sessionTitle.renameHint')}`"
      @click="start"
    >
      {{ label }}
    </button>
    <span v-else class="block truncate px-1 py-1">{{ label }}</span>
  </h1>
</template>
