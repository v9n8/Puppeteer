<script setup lang="ts">
// Settings → AI → "Response styles" (UX-17): the built-in styles (read-only,
// copyable) and the user's global styles. A session picks one; its text is added
// to the session's system prompt. Per-project styles (the table supports them)
// are managed on the project's page later (UX-19).
import { computed, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronDown, Copy, MessageSquareText, Pencil, Plus, Trash2 } from 'lucide-vue-next'
import { Badge, Button, Card, Input, Textarea } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useResponseStylesStore } from '@/stores/responseStyles'
import {
  type BuiltinStyle, STYLE_BODY_MAX, builtinStyleLabelKey, styleFormError, uniqueStyleName,
} from '@/lib/responseStyles'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const store = useResponseStylesStore()

const globalStyles = computed(() => store.styles.filter((s) => s.project_id === null))

onMounted(() => {
  store.load(null).catch((e) => toast.error(String(e)))
})

// Built-ins: one preview open at a time.
const openBuiltin = ref<string | null>(null)
function toggleBuiltin(key: string) {
  openBuiltin.value = openBuiltin.value === key ? null : key
}

// Editor: `id` null while creating. One editor open at a time.
const draft = reactive({ open: false, id: null as string | null, name: '', description: '', body: '' })
const saving = ref(false)
const draftError = computed(() => styleFormError(draft))

function openEditor(style?: { id: string | null; name: string; description: string; body: string }) {
  Object.assign(draft, { open: true, id: style?.id ?? null, name: style?.name ?? '', description: style?.description ?? '', body: style?.body ?? '' })
}

function closeEditor() {
  draft.open = false
}

async function save() {
  if (draftError.value || saving.value) return
  saving.value = true
  try {
    const input = { name: draft.name, description: draft.description, body: draft.body }
    if (draft.id) await store.update(draft.id, input)
    else await store.create({ projectId: null, ...input })
    toast.success(t('responseStyles.saved'))
    closeEditor()
  } catch (e) {
    toast.error(String(e))
  } finally {
    saving.value = false
  }
}

async function copyBuiltin(b: BuiltinStyle) {
  try {
    const style = await store.create({
      projectId: null,
      name: uniqueStyleName(t(builtinStyleLabelKey(b.key)), globalStyles.value.map((s) => s.name)),
      description: t(`responseStyles.builtin.${b.key}.description`),
      body: b.body,
    })
    toast.success(t('responseStyles.copied'))
    openEditor(style)
  } catch (e) {
    toast.error(String(e))
  }
}

async function removeStyle(id: string) {
  if (!(await confirm({
    title: t('responseStyles.deleteTitle'),
    message: t('responseStyles.deleteMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  try {
    await store.remove(id)
    if (draft.id === id) closeEditor()
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <Card body-class="p-4 space-y-6">
    <template #header>
      <MessageSquareText class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('responseStyles.title') }}</span>
    </template>

    <p class="text-2xs text-muted-foreground">{{ t('responseStyles.hint') }}</p>

    <!-- Built-in styles: read-only, copy one to edit it -->
    <section class="space-y-3">
      <div class="space-y-0.5">
        <h3 class="text-xs font-semibold text-foreground">{{ t('responseStyles.builtinTitle') }}</h3>
        <p class="text-2xs text-muted-foreground">{{ t('responseStyles.builtinHint') }}</p>
      </div>
      <div class="space-y-1.5">
        <div
          v-for="b in store.builtins"
          :key="b.key"
          class="overflow-hidden rounded-md border border-border/70 bg-muted/20"
        >
          <div class="flex items-center gap-1 pr-1.5 hover:bg-muted/40">
            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-2 py-2 pl-2 pr-3 text-left"
              :aria-expanded="openBuiltin === b.key"
              @click="toggleBuiltin(b.key)"
            >
              <ChevronDown
                class="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform"
                :class="{ '-rotate-90': openBuiltin !== b.key }"
                :stroke-width="2"
              />
              <span class="min-w-0 flex-1">
                <span class="block truncate text-xs text-foreground">{{ t(builtinStyleLabelKey(b.key)) }}</span>
                <span class="block truncate text-2xs text-muted-foreground">{{ t(`responseStyles.builtin.${b.key}.description`) }}</span>
              </span>
            </button>
            <Button
              v-if="b.body"
              variant="ghost"
              size="sm"
              class="shrink-0"
              :title="t('responseStyles.copyToMine')"
              @click="copyBuiltin(b)"
            >
              <Copy class="h-3.5 w-3.5" :stroke-width="1.75" />
              {{ t('responseStyles.copyToMine') }}
            </Button>
          </div>
          <div v-if="openBuiltin === b.key" class="border-t border-border/60 p-3">
            <pre v-if="b.body" class="whitespace-pre-wrap break-words font-mono text-2xs text-muted-foreground">{{ b.body }}</pre>
            <p v-else class="text-2xs italic text-muted-foreground">{{ t('responseStyles.noText') }}</p>
          </div>
        </div>
      </div>
    </section>

    <!-- My styles (global) -->
    <section class="space-y-3">
      <div class="flex items-center justify-between gap-2">
        <div class="space-y-0.5">
          <h3 class="text-xs font-semibold text-foreground">{{ t('responseStyles.mineTitle') }}</h3>
          <p class="text-2xs text-muted-foreground">{{ t('responseStyles.mineHint') }}</p>
        </div>
        <Button variant="outline" size="sm" class="shrink-0" @click="openEditor()">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('responseStyles.add') }}
        </Button>
      </div>

      <p
        v-if="store.loaded && !globalStyles.length && !(draft.open && !draft.id)"
        class="rounded-md border border-dashed border-border/70 px-3 py-4 text-center text-xs text-muted-foreground"
      >
        {{ t('responseStyles.empty') }}
      </p>

      <div class="space-y-1.5">
        <div
          v-for="s in globalStyles"
          :key="s.id"
          class="overflow-hidden rounded-md border bg-muted/20"
          :class="draft.open && draft.id === s.id ? 'border-primary/50' : 'border-border/70'"
        >
          <div class="flex items-center gap-1 py-1.5 pl-3 pr-1.5">
            <span class="min-w-0 flex-1">
              <span class="block truncate text-xs text-foreground">{{ s.name }}</span>
              <span v-if="s.description" class="block truncate text-2xs text-muted-foreground">{{ s.description }}</span>
            </span>
            <Badge size="xs">{{ t('responseStyles.global') }}</Badge>
            <Button variant="ghost" size="icon-sm" :title="t('common.edit')" :aria-label="t('common.edit')" @click="openEditor(s)">
              <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
            <Button
              variant="destructive-ghost"
              size="icon-sm"
              :title="t('common.delete')"
              :aria-label="t('common.delete')"
              @click="removeStyle(s.id)"
            >
              <Trash2 class="h-3.5 w-3.5" :stroke-width="2" />
            </Button>
          </div>
        </div>
      </div>

      <!-- Editor: a new style, or the highlighted one above -->
      <form
        v-if="draft.open"
        class="space-y-2 rounded-md border border-primary/40 p-3"
        @submit.prevent="save"
      >
        <h4 class="text-xs font-semibold text-foreground">
          {{ draft.id ? t('responseStyles.editTitle') : t('responseStyles.newTitle') }}
        </h4>
        <label class="block space-y-1">
          <span class="text-2xs text-muted-foreground">{{ t('responseStyles.name') }}</span>
          <Input v-model="draft.name" class="w-full" :placeholder="t('responseStyles.namePlaceholder')" />
        </label>
        <label class="block space-y-1">
          <span class="text-2xs text-muted-foreground">{{ t('responseStyles.description') }}</span>
          <Input v-model="draft.description" class="w-full" :placeholder="t('responseStyles.descriptionPlaceholder')" />
        </label>
        <label class="block space-y-1">
          <span class="flex items-center justify-between text-2xs text-muted-foreground">
            <span>{{ t('responseStyles.body') }}</span>
            <span>{{ t('responseStyles.bodyCount', { n: [...draft.body.trim()].length, max: STYLE_BODY_MAX }) }}</span>
          </span>
          <Textarea v-model="draft.body" rows="6" class="w-full font-mono" :placeholder="t('responseStyles.bodyPlaceholder')" />
        </label>
        <p v-if="draftError && (draft.name || draft.body)" class="text-2xs text-destructive">{{ t(draftError) }}</p>
        <div class="flex justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" @click="closeEditor">{{ t('common.cancel') }}</Button>
          <Button type="submit" size="sm" :disabled="!!draftError || saving">{{ t('common.save') }}</Button>
        </div>
      </form>
    </section>
  </Card>
</template>
