<script setup lang="ts">
// Agents and Commands — one list-detail screen per kind (`/agents`,
// `/commands`, routed with the `kind` prop), laid out like Rules: the sidebar
// lists the library, the right pane edits the selected item and links it into
// projects (components/libraryItems/LibraryItemDetail).
//
// The selection lives in the URL query (`?id=<item>`, `?new=1` for a blank
// draft) so a reload or a link reopens it.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { Bot, SquareSlash } from 'lucide-vue-next'
import { Badge } from '@/components/ui'
import ListDetailSidebar from '@/components/listDetail/ListDetailSidebar.vue'
import DetailEmpty from '@/components/listDetail/DetailEmpty.vue'
import LibraryItemDetail from '@/components/libraryItems/LibraryItemDetail.vue'
import { useActivatedRefresh } from '@/composables/useActivatedRefresh'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useLibraryItemsStore, type LibraryItem } from '@/stores/libraryItems'
import { useProjectsStore } from '@/stores/projects'
import { errorText } from '@/lib/featureGate'
import { buildItemRows, type LibraryKind } from '@/lib/libraryItems'

// Named explicitly: <KeepAlive :include> in App.vue matches on component name,
// and an inferred name is a build detail that minification can change. Both
// routes use this view; App.vue keys each by route name, so they cache apart.
defineOptions({ name: 'LibraryItemsView' })

const props = defineProps<{ kind: LibraryKind }>()

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const store = useLibraryItemsStore(props.kind)
const projectsStore = useProjectsStore()
const { confirm } = useConfirm()
const { toast } = useToast()

const ns = `libraryItems.${props.kind}`
const basePath = props.kind === 'agent' ? '/agents' : '/commands'
const emptyIcon = props.kind === 'agent' ? Bot : SquareSlash
const deletingId = ref<string | null>(null)

// ── Sidebar ─────────────────────────────────────────────────────────────────

const search = ref('')
const sidebarGroups = computed(() =>
  buildItemRows(store.items, { search: search.value, applied: store.applied, label: t(`${ns}.title`) }),
)
const listError = computed(() => (store.error ? errorText(store.error, t) : null))

// ── Selection (URL query) ───────────────────────────────────────────────────

const selectedId = ref<string | null>(null)
const isNew = ref(false)
const detailRef = ref<InstanceType<typeof LibraryItemDetail> | null>(null)
/** Bumped by "+ New" so a second click starts over from a blank draft. */
const draftSeq = ref(0)
/** Remounts the detail per selection, so each one starts from a clean draft. */
const detailKey = computed(() => (isNew.value ? `new-${draftSeq.value}` : selectedId.value ?? ''))
const selectedItem = computed(() => store.items.find((i) => i.id === selectedId.value) ?? null)
const itemsLoaded = ref(false)

function readQuery() {
  const id = typeof route.query.id === 'string' ? route.query.id : null
  return { id, isNew: !id && route.query.new === '1' }
}

// The view stays alive (KeepAlive) on other routes: only this screen's own
// query drives the selection, and coming back via the bare path restores it.
watch(
  () => [route.path, route.query.id, route.query.new],
  () => {
    if (route.path !== basePath) return
    const q = readQuery()
    if (q.id || q.isNew) {
      if (q.id === selectedId.value && q.isNew === isNew.value) return
      void adoptQuery(q)
    } else if (selectedId.value || isNew.value) {
      void router.replace({ query: isNew.value ? { new: '1' } : { id: selectedId.value! } })
    }
  },
  { immediate: true },
)

/** A selection arriving from outside (a link): unsaved edits are confirmed, declining restores the URL. */
async function adoptQuery(q: { id: string | null; isNew: boolean }) {
  if (!(await confirmDiscard())) {
    void router.replace({ query: isNew.value ? { new: '1' } : selectedId.value ? { id: selectedId.value } : {} })
    return
  }
  selectedId.value = q.id
  isNew.value = q.isNew
}

function setSelection(id: string | null, draft = false) {
  selectedId.value = id
  isNew.value = draft
  void router.replace({ query: draft ? { new: '1' } : id ? { id } : {} })
}

/** Leaving unsaved edits takes a confirm; true = go ahead. */
async function confirmDiscard(): Promise<boolean> {
  if (!detailRef.value?.dirty) return true
  return confirm({
    title: t('libraryItems.detail.confirmDiscardTitle'),
    message: t('libraryItems.detail.confirmDiscardMessage'),
    confirmLabel: t('libraryItems.detail.confirmDiscard'),
  })
}

async function select(id: string) {
  if (id === selectedId.value && !isNew.value) return
  if (!(await confirmDiscard())) return
  setSelection(id)
}

async function startNew() {
  if (isNew.value && !detailRef.value?.dirty) return
  if (!(await confirmDiscard())) return
  draftSeq.value++
  setSelection(null, true)
}

async function discardDraft() {
  if (!(await confirmDiscard())) return
  setSelection(null)
}

function onSaved(item: LibraryItem, created: boolean) {
  if (created) setSelection(item.id)
  // A rename moves every link: re-read where it stands.
  else void loadApplied()
}

// The selected item vanished (deleted elsewhere, or a stale ?id=).
watch(
  () => [store.items, itemsLoaded.value] as const,
  () => {
    if (itemsLoaded.value && selectedId.value && !store.error && !selectedItem.value) setSelection(null)
  },
)

// ── Data ────────────────────────────────────────────────────────────────────

function loadApplied() {
  return store.fetchApplied(projectsStore.projects.map((p) => p.id))
}

async function refresh() {
  await Promise.all([store.fetchItems(), projectsStore.fetchProjects()])
  itemsLoaded.value = true
  if (!store.error) await loadApplied()
}

onMounted(() => void refresh())

// Cached by <KeepAlive> (App.vue): re-show the screen with fresh data.
useActivatedRefresh(refresh)

// ── Actions ─────────────────────────────────────────────────────────────────

async function handleDelete(item: LibraryItem) {
  if (!(await confirm({
    title: t(`${ns}.confirmDeleteTitle`),
    message: t(`${ns}.confirmDeleteMessage`, { name: item.name }),
    confirmLabel: t('common.delete'),
  }))) return
  deletingId.value = item.id
  try {
    await store.deleteItem(item.id)
    // Its unsaved edits (if any) go with it — the delete was confirmed.
    if (selectedId.value === item.id) setSelection(null)
    toast.success(t(`${ns}.toastDeleted`))
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    deletingId.value = null
    void loadApplied()
  }
}
</script>

<template>
  <div class="flex h-full min-h-0">
    <ListDetailSidebar
      v-model:search="search"
      :groups="sidebarGroups"
      :selected-id="isNew ? null : selectedId"
      :collapse-key="`puppeteer.libraryItems.${kind}.collapsed`"
      :search-placeholder="t(`${ns}.searchPlaceholder`)"
      :loading="store.loading"
      :error="listError"
      :empty-store="store.items.length === 0"
      :empty-title="t(`${ns}.emptyTitle`)"
      :empty-hint="t(`${ns}.emptyHint`)"
      @select="select"
      @add="startNew"
      @retry="refresh"
    >
      <template #row="{ row, selected }">
        <span class="flex min-w-0 items-center gap-1.5">
          <span class="min-w-0 flex-1 truncate font-mono text-sm font-medium leading-snug" :class="selected && 'text-foreground'">
            {{ row.item.name }}
          </span>
          <Badge
            v-if="row.projects > 0"
            :tone="row.drifted > 0 ? 'warning' : 'neutral'"
            size="xs"
            class="shrink-0"
            :title="row.drifted > 0
              ? t('libraryItems.list.projectsDrifted', { count: row.projects, drifted: row.drifted })
              : t('libraryItems.list.projectsTitle', { count: row.projects })"
          >
            {{ row.projects }}
          </Badge>
        </span>
        <span class="mt-1 truncate text-2xs text-muted-foreground/70">{{ row.item.description }}</span>
      </template>
    </ListDetailSidebar>

    <LibraryItemDetail
      v-if="isNew || selectedItem"
      ref="detailRef"
      :key="detailKey"
      :kind="kind"
      :item-id="isNew ? null : selectedId"
      :item="isNew ? null : selectedItem"
      :projects="projectsStore.projects"
      :deleting="!!selectedId && deletingId === selectedId"
      @saved="onSaved"
      @delete="selectedItem && handleDelete(selectedItem)"
      @discard="discardDraft"
      @missing="setSelection(null)"
    />
    <DetailEmpty v-else :icon="emptyIcon" :text="t(`${ns}.noSelection`)" />
  </div>
</template>
