<script setup lang="ts">
// Settings → AI → "Response styles" (UX-17, UX-36): one list of the global
// styles, the samples included; each can be edited or deleted, and "Restore
// sample styles" puts back the deleted samples. "None" adds no text, so it is
// listed but locked. A session picks a style; its text is added to the
// session's system prompt. With `projectId` (the project page, UX-19) it
// manages that project's own styles instead.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Lock, MessageSquareText, Pencil, Plus, RotateCcw, Trash2 } from 'lucide-vue-next'
import { Badge, Button, Card, InfoTip } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useResponseStylesStore } from '@/stores/responseStyles'
import { type ResponseStyle, missingSamples, styleText } from '@/lib/responseStyles'
import ResponseStyleModal from './ResponseStyleModal.vue'

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

// Editor dialog (ResponseStyleModal): `editing` null while creating.
const editorOpen = ref(false)
const editing = ref<ResponseStyle | null>(null)

function openEditor(style?: ResponseStyle) {
  editing.value = style ?? null
  editorOpen.value = true
}

async function removeStyle(style: ResponseStyle) {
  if (!(await confirm({
    title: t('responseStyles.deleteTitle'),
    message: t(style.builtin_key ? 'responseStyles.deleteSampleMessage' : 'responseStyles.deleteMessage'),
    confirmLabel: t('common.delete'),
  }))) return
  try {
    await store.remove(style.id)
    if (editing.value?.id === style.id) editorOpen.value = false
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
      <template v-if="!scope">
        <InfoTip :text="t('responseStyles.hint')" />
      </template>
      <Button v-else variant="outline" size="sm" class="ml-auto" @click="openEditor()">
        <Plus class="h-3.5 w-3.5" :stroke-width="2" />
        {{ t('responseStyles.projectAdd') }}
      </Button>
    </template>

    <div v-if="!scope" class="flex flex-wrap items-center justify-between gap-2" data-settings-id="styles-mine">
      <div class="flex min-w-0 items-center gap-1.5">
        <h3 class="text-xs font-semibold text-foreground">{{ t('responseStyles.mineTitle') }}</h3>
        <InfoTip :text="t('responseStyles.mineHint')" />
      </div>
      <div class="flex shrink-0 items-center gap-1.5">
        <Button variant="ghost" size="sm" :disabled="!missing.length" @click="restoreSamples">
          <RotateCcw class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('responseStyles.restore') }}
        </Button>
        <InfoTip :text="t('responseStyles.restoreHint')" />
        <Button variant="outline" size="sm" @click="openEditor()">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('responseStyles.add') }}
        </Button>
      </div>
    </div>

    <p
      v-if="scope && store.loaded && !ownStyles.length"
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
        class="overflow-hidden rounded-md border border-border/70 bg-muted/20"
      >
        <div class="flex items-center gap-1 py-1.5 pl-3 pr-1.5">
          <span class="min-w-0 flex-1">
            <span class="block truncate text-xs text-foreground" :title="r.name">{{ r.name }}</span>
            <span v-if="r.description" class="block truncate text-2xs text-muted-foreground" :title="r.description">{{ r.description }}</span>
          </span>
          <Badge v-if="r.style.builtin_key" size="xs">{{ t('responseStyles.sampleBadge') }}</Badge>
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
  </Card>

  <ResponseStyleModal v-model:open="editorOpen" :target="editing" :project-id="scope" />
</template>
