<script setup lang="ts">
// Settings → AI → "Response styles" (UX-17, UX-36): one list of the global
// styles, the samples included; each can be edited or deleted, and "Restore
// sample styles" puts back the deleted samples. "None" adds no text, so it is
// listed but locked. A session picks a style; its text is added to the
// session's system prompt. With `projectId` (the project page, UX-19) it
// manages that project's own styles instead.
import { computed, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Lock, MessageSquareText, Pencil, Plus, RotateCcw, Trash2 } from 'lucide-vue-next'
import { Badge, Button, Card, InfoTip, Input, Textarea } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useResponseStylesStore } from '@/stores/responseStyles'
import { type ResponseStyle, STYLE_BODY_MAX, missingSamples, styleFormError, styleText } from '@/lib/responseStyles'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const store = useResponseStylesStore()
const props = defineProps<{ projectId?: string | null }>()
const scope = computed(() => props.projectId ?? null)

// The styles this screen edits: the global ones, or the project's own.
const ownStyles = computed(() => store.styles.filter((s) => s.project_id === scope.value))
// With the texts shown: a sample's untouched name and description translated.
const rows = computed(() => ownStyles.value.map((style) => ({ style, ...styleText(style, t) })))
const missing = computed(() => (scope.value ? [] : missingSamples(store.styles)))

watch(scope, (id) => {
  store.load(id).catch((e) => toast.error(String(e)))
}, { immediate: true })

// Editor: `id` null while creating. One editor open at a time. A sample opens
// with its shown (translated) texts; saving one unchanged keeps the stored text,
// so it goes on following the UI language.
const draft = reactive({ open: false, id: null as string | null, name: '', description: '', body: '' })
let editing: ResponseStyle | null = null
const saving = ref(false)
const draftError = computed(() => styleFormError(draft))

function openEditor(style?: ResponseStyle) {
  editing = style ?? null
  const text = style ? styleText(style, t) : { name: '', description: '' }
  Object.assign(draft, { open: true, id: style?.id ?? null, ...text, body: style?.body ?? '' })
}

function closeEditor() {
  draft.open = false
  editing = null
}

async function save() {
  if (draftError.value || saving.value) return
  saving.value = true
  try {
    const input = { name: draft.name, description: draft.description, body: draft.body }
    if (editing) {
      const shown = styleText(editing, t)
      if (editing.stock_name && input.name.trim() === shown.name) input.name = editing.name
      if (editing.stock_description && input.description.trim() === shown.description) input.description = editing.description
    }
    if (draft.id) await store.update(draft.id, input)
    else await store.create({ projectId: scope.value, ...input })
    toast.success(t('responseStyles.saved'))
    closeEditor()
  } catch (e) {
    toast.error(String(e))
  } finally {
    saving.value = false
  }
}

async function removeStyle(style: ResponseStyle) {
  if (!(await confirm({
    title: t('responseStyles.deleteTitle'),
    message: t(style.builtin_key ? 'responseStyles.deleteSampleMessage' : 'responseStyles.deleteMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  try {
    await store.remove(style.id)
    if (draft.id === style.id) closeEditor()
  } catch (e) {
    toast.error(String(e))
  }
}

async function restoreSamples() {
  try {
    const restored = await store.restoreSamples()
    toast.success(t('responseStyles.restored', { n: restored.length }))
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <Card body-class="p-4 space-y-3">
    <template #header>
      <MessageSquareText class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t(scope ? 'responseStyles.projectTitle' : 'responseStyles.title') }}</span>
      <InfoTip :text="t(scope ? 'responseStyles.projectHint' : 'responseStyles.hint')" />
    </template>

    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="flex min-w-0 items-center gap-1.5">
        <h3 class="text-xs font-semibold text-foreground">{{ t(scope ? 'responseStyles.projectStylesTitle' : 'responseStyles.mineTitle') }}</h3>
        <InfoTip :text="t(scope ? 'responseStyles.projectStylesHint' : 'responseStyles.mineHint')" />
      </div>
      <div class="flex shrink-0 items-center gap-1.5">
        <template v-if="!scope">
          <Button variant="ghost" size="sm" :disabled="!missing.length" @click="restoreSamples">
            <RotateCcw class="h-3.5 w-3.5" :stroke-width="2" />
            {{ t('responseStyles.restore') }}
          </Button>
          <InfoTip :text="t('responseStyles.restoreHint')" />
        </template>
        <Button variant="outline" size="sm" @click="openEditor()">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('responseStyles.add') }}
        </Button>
      </div>
    </div>

    <p
      v-if="scope && store.loaded && !ownStyles.length && !(draft.open && !draft.id)"
      class="rounded-md border border-dashed border-border/70 px-3 py-4 text-center text-xs text-muted-foreground"
    >
      {{ t('responseStyles.projectEmpty') }}
    </p>

    <div class="space-y-1.5">
      <!-- "None": adds no text, so there is nothing to edit and it cannot be deleted -->
      <div v-if="!scope" class="flex items-center gap-1 rounded-md border border-border/70 bg-muted/20 py-1.5 pl-3 pr-1.5">
        <span class="min-w-0 flex-1">
          <span class="block truncate text-xs text-foreground" :title="t('settings.ai.styleNone')">{{ t('settings.ai.styleNone') }}</span>
          <span class="block truncate text-2xs text-muted-foreground" :title="t('settings.ai.styleNoneHint')">{{ t('settings.ai.styleNoneHint') }}</span>
        </span>
        <span class="inline-flex h-7 w-7 items-center justify-center text-muted-foreground" :title="t('responseStyles.noneLocked')">
          <Lock class="h-3.5 w-3.5" :stroke-width="1.75" />
        </span>
      </div>
      <div
        v-for="r in rows"
        :key="r.style.id"
        class="overflow-hidden rounded-md border bg-muted/20"
        :class="draft.open && draft.id === r.style.id ? 'border-primary/50' : 'border-border/70'"
      >
        <div class="flex items-center gap-1 py-1.5 pl-3 pr-1.5">
          <span class="min-w-0 flex-1">
            <span class="block truncate text-xs text-foreground" :title="r.name">{{ r.name }}</span>
            <span v-if="r.description" class="block truncate text-2xs text-muted-foreground" :title="r.description">{{ r.description }}</span>
          </span>
          <Badge v-if="r.style.builtin_key" size="xs">{{ t('responseStyles.sampleBadge') }}</Badge>
          <Badge v-if="scope" size="xs">{{ t('responseStyles.projectBadge') }}</Badge>
          <Button variant="ghost" size="icon-sm" :title="t('common.edit')" :aria-label="t('common.edit')" @click="openEditor(r.style)">
            <Pencil class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
          <Button
            variant="destructive-ghost"
            size="icon-sm"
            :title="t('common.delete')"
            :aria-label="t('common.delete')"
            @click="removeStyle(r.style)"
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
  </Card>
</template>
