<script setup lang="ts">
// Settings → "Prompt Templates": the saved prompt library, edited as a list here
// and stored as a JSON string in the `saved_prompts` setting.
import { onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronDown, FileText, GripVertical, Pencil, Plus, Trash2 } from 'lucide-vue-next'
import { Button, Card, Input, Textarea } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useDragSort } from '@/composables/useDragSort'
import {
  type SavedPrompt, newPromptId, parseSavedPrompts, serializeSavedPrompts, promptLabel,
} from '@/lib/savedPrompts'
import { useSettingsForm } from './useSettingsForm'

const { t } = useI18n()
const { confirm } = useConfirm()
const { settings, loading, persistChanges } = useSettingsForm()

// The generic auto-save waits 400ms, which is far too eager while typing a
// multi-line prompt body — it fires (and toasts) between words. Give this field
// its own, longer debounce and persist from it directly.
// Sections mount once the settings are loaded, so this reads the stored library.
const savedPrompts = ref<SavedPrompt[]>(parseSavedPrompts(settings.value.saved_prompts))
const SAVED_PROMPTS_DEBOUNCE_MS = 1500
let savedPromptsTimer: ReturnType<typeof setTimeout> | null = null

function flushSavedPrompts() {
  savedPromptsTimer = null
  const next = serializeSavedPrompts(savedPrompts.value)
  if (next === settings.value.saved_prompts) return
  settings.value.saved_prompts = next
  // Persist directly: the form's `watch(settings)` is stopped once the view
  // unmounts, so relying on it would silently drop a flush that lands after
  // navigation. It re-runs harmlessly — persistChanges() no-ops when nothing differs.
  persistChanges()
}

watch(savedPrompts, () => {
  if (loading.value) return
  if (savedPromptsTimer) clearTimeout(savedPromptsTimer)
  savedPromptsTimer = setTimeout(flushSavedPrompts, SAVED_PROMPTS_DEBOUNCE_MS)
}, { deep: true })

// Library items render as a collapsed list and expand only when clicked, so a
// long library stays scannable. Only one row is open at a time.
const expandedPromptId = ref<string | null>(null)

function togglePrompt(id: string) {
  expandedPromptId.value = expandedPromptId.value === id ? null : id
}

// Drag a row's grip to reorder; the array order is the order the composer
// picker lists them in.
const savedPromptsListRef = ref<HTMLElement | null>(null)
const { dragFrom: promptDragFrom, start: startPromptDrag, itemStyle: promptRowStyle } = useDragSort({
  container: savedPromptsListRef,
  onDrop: (from, to) => {
    const list = savedPrompts.value
    const [moved] = list.splice(from, 1)
    if (moved) list.splice(to, 0, moved)
  },
})

function addSavedPrompt() {
  const id = newPromptId()
  savedPrompts.value.push({ id, title: '', body: '' })
  expandedPromptId.value = id // open the empty row so it can be filled in
}

async function removeSavedPrompt(id: string) {
  if (!(await confirm({
    title: t('settings.prompts.deleteTitle'),
    message: t('settings.prompts.deleteMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  savedPrompts.value = savedPrompts.value.filter((p) => p.id !== id)
  if (expandedPromptId.value === id) expandedPromptId.value = null
}

onUnmounted(() => {
  // Don't lose a prompt edit made within the debounce window before leaving.
  if (savedPromptsTimer) {
    clearTimeout(savedPromptsTimer)
    flushSavedPrompts()
  }
})
</script>

<template>
  <Card body-class="p-4 space-y-6">
    <template #header>
      <FileText class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.prompts.title') }}</span>
    </template>

      <!-- Reusable prompt library, picked from the composer dropdown in a run -->
      <section class="space-y-3">
        <div class="flex items-center justify-between gap-2">
          <div class="space-y-0.5">
            <h3 class="text-xs font-semibold text-foreground">{{ t('settings.prompts.library') }}</h3>
            <p class="text-2xs text-muted-foreground">{{ t('settings.prompts.libraryHint') }}</p>
          </div>
          <Button variant="outline" size="sm" class="shrink-0" @click="addSavedPrompt">
            <Plus class="h-3.5 w-3.5" :stroke-width="2" />
            {{ t('settings.prompts.addPrompt') }}
          </Button>
        </div>

        <p v-if="!savedPrompts.length" class="rounded-md border border-dashed border-border/70 px-3 py-4 text-center text-xs text-muted-foreground">
          {{ t('settings.prompts.libraryEmpty') }}
        </p>

        <div ref="savedPromptsListRef" class="space-y-1.5">
          <div
            v-for="(p, i) in savedPrompts"
            :key="p.id"
            :data-drag-index="i"
            class="relative overflow-hidden rounded-md border border-border/70"
            :class="promptDragFrom === i ? 'bg-background shadow-xl ring-1 ring-primary/40' : 'bg-muted/20'"
            :style="promptRowStyle(i)"
          >
            <!-- Collapsed row: grip + toggle on the left, edit + delete actions on the right -->
            <div class="flex items-center gap-1 pr-1.5 transition-colors hover:bg-muted/40">
              <button
                type="button"
                class="ml-1 flex h-6 w-5 shrink-0 items-center justify-center rounded text-muted-foreground/60 hover:text-foreground hover:bg-accent cursor-grab active:cursor-grabbing touch-none"
                :title="t('settings.prompts.dragToReorder')"
                :aria-label="t('settings.prompts.dragToReorder')"
                @pointerdown="startPromptDrag(i, $event)"
              >
                <GripVertical class="h-3.5 w-3.5" :stroke-width="2" />
              </button>
              <button
                type="button"
                class="flex min-w-0 flex-1 items-center gap-2 py-2 pl-1 pr-3 text-left"
                :aria-expanded="expandedPromptId === p.id"
                @click="togglePrompt(p.id)"
              >
                <ChevronDown
                  class="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform"
                  :class="{ '-rotate-90': expandedPromptId !== p.id }"
                  :stroke-width="2"
                />
                <span
                  class="min-w-0 flex-1 truncate text-xs"
                  :class="promptLabel(p) ? 'text-foreground' : 'italic text-muted-foreground'"
                >
                  {{ promptLabel(p) || t('settings.prompts.untitled', { n: i + 1 }) }}
                </span>
              </button>
              <Button
                variant="ghost"
                size="icon-sm"
                :title="t('common.edit')"
                :aria-label="t('common.edit')"
                @click="togglePrompt(p.id)"
              >
                <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
              </Button>
              <Button
                variant="destructive-ghost"
                size="icon-sm"
                :title="t('common.delete')"
                :aria-label="t('common.delete')"
                @click="removeSavedPrompt(p.id)"
              >
                <Trash2 class="h-3.5 w-3.5" :stroke-width="2" />
              </Button>
            </div>

            <!-- Expanded editor: title and body share the same full width -->
            <div v-if="expandedPromptId === p.id" class="space-y-1.5 border-t border-border/60 p-3">
              <Input
                v-model="p.title"
                class="w-full"
                :placeholder="t('settings.prompts.titlePlaceholder', { n: i + 1 })"
              />
              <Textarea
                v-model="p.body"
                rows="3"
                class="w-full font-mono"
                :placeholder="t('settings.prompts.bodyPlaceholder')"
              />
            </div>
          </div>
        </div>
      </section>
  </Card>
</template>
