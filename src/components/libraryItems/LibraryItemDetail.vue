<script setup lang="ts">
// Right pane of the Agents / Commands screens: one item (or a new draft when
// `itemId` is null), laid out like RuleDetail. The markdown file is the source
// of truth for the description, and for an agent's name too; a command's name
// is its file name, so it is a field of its own (lib/libraryItems).
//
// Two tabs: the file, and the projects the item is linked into (Claude only:
// `.claude/agents` / `.claude/commands`). The host keys this component by the
// selection and asks `dirty` before switching.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  AlertCircle, Columns2, Eye, FileText, FolderOpen, LayoutTemplate, MoreHorizontal, Pencil, Trash2, X,
} from 'lucide-vue-next'
import {
  Badge, Banner, Button, DropdownItem, DropdownMenu, DropdownSeparator, Input, SegmentedControl, ToggleSwitch,
} from '@/components/ui'
import SkillEditor from '@/components/SkillEditor.vue'
import DetailHeader from '@/components/listDetail/DetailHeader.vue'
import DetailSection from '@/components/listDetail/DetailSection.vue'
import FindBar from '@/components/FindBar.vue'
import { useLibraryItemsStore, type LibraryItem } from '@/stores/libraryItems'
import type { Project } from '@/stores/projects'
import { useToast } from '@/composables/useToast'
import { useSelectionSurface } from '@/composables/useSelectionSurface'
import { usePageFind } from '@/composables/usePageFind'
import { SCREEN_SELECTION } from '@/lib/selectionScreens'
import { applyMermaidFence, vMermaid } from '@/lib/mermaid'
import { vCopyCode } from '@/lib/copyCode'
import { errorText } from '@/lib/featureGate'
import { relativeAge } from '@/lib/itemText'
import { formatFullDate } from '@/lib/slackThreadUtils'
import { parseFrontmatter } from '@/lib/rulesRows'
import {
  itemDescription, itemName, projectLinks, setItemField, templateOf, validateItem,
  type LibraryKind, type LinkState,
} from '@/lib/libraryItems'

const props = defineProps<{
  kind: LibraryKind
  /** null = a new, unsaved item. */
  itemId: string | null
  /** The list entry of the item (meta line, external-change detection). */
  item: LibraryItem | null
  projects: Project[]
  deleting: boolean
}>()

const emit = defineEmits<{
  saved: [item: LibraryItem, created: boolean]
  delete: []
  discard: []
  /** The item could not be loaded (deleted, stale id): the host drops the selection. */
  missing: []
}>()

const { t } = useI18n()
const store = useLibraryItemsStore(props.kind)
const { toast } = useToast()
/** i18n prefix of the strings that differ per kind. */
const ns = `libraryItems.${props.kind}`
const template = templateOf(props.kind)

const isNew = computed(() => props.itemId === null)
const content = ref(template)
/** What was last loaded or saved; the draft is dirty when it differs. */
const original = ref(template)
/** A command's name field (an agent's lives in the frontmatter). */
const nameField = ref('')
const originalName = ref('')
const loading = ref(false)
/** Loading failed: the draft is still the template, so it must never be saved over the item. */
const loadFailed = ref(false)
const saving = ref(false)
const editorRef = ref<InstanceType<typeof SkillEditor> | null>(null)

const name = computed(() => itemName(props.kind, content.value, nameField.value))
const validationError = computed(() => {
  const err = validateItem(props.kind, content.value, nameField.value)
  return err ? t(`libraryItems.editor.${err}`) : null
})
const dirty = computed(
  () => content.value !== original.value || (props.kind === 'command' && nameField.value.trim() !== originalName.value),
)
const canSave = computed(
  () => dirty.value && !saving.value && !loading.value && !loadFailed.value && !validationError.value,
)

const meta = computed(() =>
  props.item ? t('libraryItems.detail.metaUpdated', { age: relativeAge(props.item.updated_at) }) : t('libraryItems.detail.metaDraft'),
)
const metaTitle = computed(() =>
  props.item ? `${t('libraryItems.detail.metaUpdated', { age: formatFullDate(props.item.updated_at) })}\n${props.item.source_path}` : '',
)
const fileName = computed(() => `${name.value || props.kind}.md`)

async function load() {
  if (!props.itemId) return
  loading.value = true
  try {
    const ic = await store.getItem(props.itemId)
    content.value = ic.content
    original.value = ic.content
    nameField.value = ic.item.name
    originalName.value = ic.item.name
    loadFailed.value = false
  } catch (e) {
    loadFailed.value = true
    toast.error(errorText(e, t))
    emit('missing')
  } finally {
    loading.value = false
  }
}

onMounted(load)

// Changed elsewhere (another window) while untouched here: show it.
watch(
  () => props.item?.updated_at,
  (now, before) => {
    if (now && before && now !== before && !dirty.value && !saving.value) void load()
  },
)

// ── Metadata fields ─────────────────────────────────────────────────────────
// Local drafts: the frontmatter stores trimmed values, so binding the inputs to
// the parsed values directly would eat a space while it is being typed.
const descriptionDraft = ref('')

watch(content, (raw) => {
  const fm = parseFrontmatter(raw)
  if (props.kind === 'agent' && fm.name !== nameField.value.trim()) nameField.value = fm.name
  if (fm.description !== descriptionDraft.value.trim()) descriptionDraft.value = fm.description
}, { immediate: true })

function setName(value: string) {
  nameField.value = value
  if (props.kind === 'agent') content.value = setItemField(content.value, 'name', value.trim())
}

function setDescription(value: string) {
  descriptionDraft.value = value
  content.value = setItemField(content.value, 'description', value.trim())
}

async function save() {
  if (!canSave.value) return
  const payload = { name: name.value, description: itemDescription(content.value), content: content.value }
  saving.value = true
  try {
    const saved = props.itemId
      ? await store.updateItem({ id: props.itemId, ...payload })
      : await store.createItem(payload)
    original.value = payload.content
    originalName.value = payload.name
    toast.success(t(`${ns}.toastSaved`))
    emit('saved', saved, !props.itemId)
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    saving.value = false
  }
}

function onKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
    e.preventDefault()
    void save()
  }
}

// ── Tabs: the file, or its projects ─────────────────────────────────────────

const tab = ref<'file' | 'projects'>('file')

type ViewMode = 'edit' | 'split' | 'preview'
const VIEW_KEY = 'puppeteer.libraryItems.view'
const viewMode = ref<ViewMode>(readViewMode())
const viewModes = computed(() => [
  { value: 'edit' as const, label: t('libraryItems.detail.markdown'), icon: Pencil },
  { value: 'split' as const, label: t('libraryItems.detail.split'), icon: Columns2 },
  { value: 'preview' as const, label: t('libraryItems.detail.preview'), icon: Eye },
])

function readViewMode(): ViewMode {
  try {
    const v = localStorage.getItem(VIEW_KEY)
    return v === 'edit' || v === 'preview' ? v : 'split'
  } catch {
    return 'split'
  }
}

function setViewMode(v: ViewMode) {
  viewMode.value = v
  try {
    localStorage.setItem(VIEW_KEY, v)
  } catch {
    /* storage unavailable */
  }
}

const previewHtml = ref('')
let previewSeq = 0
async function updatePreview(md: string) {
  const seq = ++previewSeq
  let html: string
  try {
    const MarkdownIt = (await import('markdown-it')).default
    const mdi = new MarkdownIt({ html: false, linkify: true, typographer: true })
    applyMermaidFence(mdi)
    html = mdi.render(md)
  } catch {
    html = `<pre>${md.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c] ?? c)}</pre>`
  }
  if (seq === previewSeq) previewHtml.value = html
}

// Only render while a preview is on screen.
watch([content, viewMode], ([md, mode]) => {
  if (mode !== 'edit') void updatePreview(md)
}, { immediate: true })

// Selected text in the preview can be translated, or copied as markdown
// mapped back onto the editor's text.
const previewEl = ref<HTMLElement | null>(null)
useSelectionSurface({ el: previewEl, allow: SCREEN_SELECTION.libraryItem, markdownSource: () => [content.value] })

// ⌘F searches the preview; a re-render detaches the old matches, and the
// edit-only view (or the projects tab) has nothing to search.
const find = usePageFind(previewEl)
const findOpen = find.open
watch([previewHtml, viewMode, tab], () =>
  viewMode.value === 'edit' || tab.value !== 'file' ? find.closeFind() : find.refresh(),
)

// ── Projects ────────────────────────────────────────────────────────────────

const links = computed(() => (props.itemId ? projectLinks(props.projects, store.applied, props.itemId) : []))
const linkedCount = computed(() => links.value.filter((l) => l.state !== null).length)
const togglingId = ref<string | null>(null)

const STATE_TONE: Record<LinkState, 'success' | 'warning' | 'neutral'> = {
  installed: 'success',
  drifted: 'warning',
  git_managed: 'neutral',
}

async function toggleProject(project: Project, on: boolean) {
  if (!props.itemId || togglingId.value) return
  togglingId.value = project.id
  try {
    if (on) await store.applyToProject(project.id, props.itemId)
    else await store.removeFromProject(project.id, props.itemId)
    toast.success(t(on ? 'libraryItems.detail.toastLinked' : 'libraryItems.detail.toastUnlinked', { project: project.name }))
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    togglingId.value = null
  }
}

defineExpose({ dirty, save })
</script>

<template>
  <section class="flex min-w-0 flex-1 flex-col" @keydown="onKeydown">
    <DetailHeader :title="name" :placeholder="t(`${ns}.untitled`)">
      <template #meta>
        <span :title="metaTitle">{{ meta }}</span>
      </template>
      <template #actions>
        <Button size="sm" :disabled="!canSave" :title="t('libraryItems.detail.saveHint')" @click="save">
          {{ saving ? t('libraryItems.detail.saving') : t('common.save') }}
        </Button>
        <DropdownMenu align="right">
          <template #trigger>
            <Button variant="outline" size="icon" :title="t('libraryItems.detail.moreActions')" :aria-label="t('libraryItems.detail.moreActions')">
              <MoreHorizontal class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
          </template>
          <DropdownItem :disabled="loading" @click="tab = 'file'; editorRef?.insertTemplate()">
            <LayoutTemplate class="h-3.5 w-3.5" :stroke-width="1.75" />
            {{ t('libraryItems.detail.insertTemplate') }}
          </DropdownItem>
          <DropdownSeparator />
          <DropdownItem v-if="isNew" variant="destructive" @click="emit('discard')">
            <X class="h-3.5 w-3.5" :stroke-width="1.75" />
            {{ t('libraryItems.detail.discardDraft') }}
          </DropdownItem>
          <DropdownItem v-else variant="destructive" :disabled="deleting" @click="emit('delete')">
            <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
            {{ t('common.delete') }}
          </DropdownItem>
        </DropdownMenu>
      </template>
    </DetailHeader>

    <div v-if="loading || loadFailed" class="flex flex-1 items-center justify-center text-sm text-muted-foreground">
      {{ t('common.loading') }}
    </div>

    <template v-else>
      <div class="shrink-0 px-5">
        <DetailSection :title="t('libraryItems.detail.details')" :hint="t(`${ns}.detailsHint`)">
          <div class="grid grid-cols-1 gap-3">
            <label class="block space-y-1">
              <span class="text-2xs font-medium text-muted-foreground">{{ t('libraryItems.detail.name') }}</span>
              <Input
                :model-value="nameField"
                class="font-mono"
                :placeholder="t(`${ns}.namePlaceholder`)"
                @update:model-value="setName"
              />
            </label>
            <label class="block space-y-1">
              <span class="text-2xs font-medium text-muted-foreground">{{ t('libraryItems.detail.description') }}</span>
              <Input
                :model-value="descriptionDraft"
                :placeholder="t(`${ns}.descriptionPlaceholder`)"
                @update:model-value="setDescription"
              />
            </label>
          </div>
        </DetailSection>
      </div>

      <!-- File tab, projects tab + view mode -->
      <div class="flex shrink-0 items-center gap-2 border-y border-border/60 bg-muted/30 px-3 py-1">
        <div class="flex min-w-0 flex-1 items-center gap-0.5">
          <button
            type="button"
            class="flex shrink-0 cursor-pointer items-center gap-1 rounded px-2 py-1 font-mono text-2xs transition-colors"
            :class="tab === 'file' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'"
            :title="item?.source_path ?? fileName"
            @click="tab = 'file'"
          >
            <FileText class="h-3 w-3 shrink-0" :stroke-width="1.75" />
            <span class="max-w-48 truncate">{{ fileName }}</span>
            <span v-if="dirty" class="h-1.5 w-1.5 rounded-full bg-primary" />
          </button>
          <button
            v-if="!isNew"
            type="button"
            class="flex shrink-0 cursor-pointer items-center gap-1 rounded px-2 py-1 text-2xs transition-colors"
            :class="tab === 'projects' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'"
            @click="tab = 'projects'"
          >
            <FolderOpen class="h-3 w-3 shrink-0" :stroke-width="1.75" />
            {{ t('libraryItems.detail.projects', { count: linkedCount }) }}
          </button>
        </div>
        <SegmentedControl
          v-if="tab === 'file'"
          :model-value="viewMode"
          :options="viewModes"
          :stretch="false"
          icon-only
          @update:model-value="setViewMode"
        />
      </div>

      <Banner
        v-if="validationError && tab === 'file'"
        tone="warning"
        :icon="AlertCircle"
        class="shrink-0 rounded-none border-x-0 border-t-0 px-3 text-xs"
      >
        <p class="truncate" :title="validationError">{{ validationError }}</p>
      </Banner>

      <!-- The file: kept mounted on the projects tab so its undo history survives. -->
      <div v-show="tab === 'file'" class="relative flex min-h-0 flex-1">
        <div
          v-show="viewMode !== 'preview'"
          class="flex min-w-0 flex-1 flex-col overflow-hidden"
          :class="viewMode === 'split' && 'border-r border-border/60'"
        >
          <SkillEditor
            ref="editorRef"
            v-model="content"
            :is-dark="true"
            :template="template"
            class="flex-1"
          />
        </div>
        <FindBar v-if="findOpen" :find="find" class="absolute top-2 right-4 z-20" />
        <div v-if="viewMode !== 'edit'" class="min-w-0 flex-1 overflow-auto p-5">
          <div
            v-mermaid
            v-copy-code
            class="text-sm leading-relaxed
              [&_h1]:text-lg [&_h1]:font-bold [&_h1]:mb-3 [&_h1]:mt-0
              [&_h2]:text-base [&_h2]:font-semibold [&_h2]:mb-2 [&_h2]:mt-4
              [&_h3]:text-sm [&_h3]:font-semibold [&_h3]:mb-1.5 [&_h3]:mt-3
              [&_p]:mb-3 [&_p]:text-foreground/90
              [&_ul]:list-disc [&_ul]:pl-5 [&_ul]:mb-3
              [&_ol]:list-decimal [&_ol]:pl-5 [&_ol]:mb-3
              [&_li]:mb-1
              [&_code]:font-mono [&_code]:bg-muted [&_code]:px-1 [&_code]:rounded [&_code]:text-xs [&_code]:text-foreground/80
              [&_pre]:bg-muted [&_pre]:p-3 [&_pre]:rounded-md [&_pre]:overflow-auto [&_pre]:mb-3 [&_pre]:border [&_pre]:border-border/50
              [&_blockquote]:border-l-2 [&_blockquote]:border-primary/40 [&_blockquote]:pl-4 [&_blockquote]:text-muted-foreground [&_blockquote]:mb-3
              [&_hr]:border-border/60 [&_hr]:my-4
              [&_a]:text-primary [&_a]:underline [&_a]:underline-offset-2
              [&_strong]:font-semibold"
            ref="previewEl"
            v-html="previewHtml"
          />
        </div>
      </div>

      <!-- Projects: one switch each links the saved file into that project. -->
      <div v-if="tab === 'projects'" class="min-h-0 flex-1 overflow-auto px-5 py-3">
        <p class="mb-3 text-2xs text-muted-foreground">{{ t(`${ns}.projectsHint`) }}</p>
        <p v-if="links.length === 0" class="text-xs text-muted-foreground">{{ t('libraryItems.detail.noProjects') }}</p>
        <ul v-else class="divide-y divide-border/60">
          <li v-for="link in links" :key="link.project.id" class="flex items-center gap-3 py-2">
            <div class="min-w-0 flex-1">
              <p class="truncate text-sm font-medium">{{ link.project.name }}</p>
              <p class="truncate font-mono text-2xs text-muted-foreground/70" :title="link.project.path">{{ link.project.path }}</p>
            </div>
            <Badge
              v-if="link.state"
              :tone="STATE_TONE[link.state]"
              size="xs"
              class="shrink-0"
              :title="t(`libraryItems.detail.stateHint.${link.state}`)"
            >
              {{ t(`libraryItems.detail.state.${link.state}`) }}
            </Badge>
            <ToggleSwitch
              :checked="link.state !== null"
              :loading="togglingId === link.project.id"
              :disabled="togglingId !== null"
              :label="link.project.name"
              @toggle="toggleProject(link.project, link.state === null)"
            />
          </li>
        </ul>
      </div>
    </template>
  </section>
</template>
