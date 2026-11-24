<script setup lang="ts">
// Settings › Flag (V3-20): the shared session flags. Add / edit in a dialog, change a
// colour from the row's dot, delete with a confirm, reorder by pointer or keyboard,
// and put back the deleted defaults. Every write goes through the sessionFlags store,
// which reloads this list and tells the other windows.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Flag, Plus, RotateCcw } from 'lucide-vue-next'
import { Banner, Button, Card, InfoTip } from '@/components/ui'
import FlagRow from './FlagRow.vue'
import FlagEditorModal from '@/components/flags/FlagEditorModal.vue'
import { useFlagTargets } from '@/components/sessions/useFlagTargets'
import { useDragSort } from '@/composables/useDragSort'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import { useProjectsStore } from '@/stores/projects'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { keyboardMove, moveId, restoreDisabled } from '@/lib/flagEditor'
import { flagLabel, type SessionFlag } from '@/lib/sessionFlags'
import type { ProjectColor } from '@/lib/projectAppearance'
import SettingsSectionHead from './SettingsSectionHead.vue'

const ROW_PX = 44
const LINK_MARK = '\u0001'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const store = useSessionFlagsStore()
const projects = useProjectsStore()
const tabs = useWorkspaceTabsStore()
const { canOpenProjectFlags, openProjectFlags } = useFlagTargets()

const loaded = ref(false)
const loadError = ref<string | null>(null)

async function load() {
  loadError.value = null
  try {
    await store.load(null)
    loaded.value = true
  } catch (e) {
    loadError.value = String(e)
  }
}
onMounted(() => {
  void load()
  if (!projects.projects.length) void projects.fetchProjects()
})

// The order on screen: the store's, unless a move is in flight (kept here until it is saved).
const localIds = ref<string[] | null>(null)
const flags = computed<SessionFlag[]>(() => {
  const list = store.flagsOf(null)
  const ids = localIds.value
  if (!ids) return list
  const byId = new Map(list.map((f) => [f.id, f]))
  return ids.map((id) => byId.get(id)).filter((f): f is SessionFlag => !!f)
})
const noRestore = computed(() => restoreDisabled(store.flagsOf(null)))

// "Project › Flag" opens the Flag tab of the project used last.
const lastProjectId = computed(() => tabs.recent.find((id) => projects.projects.some((p) => p.id === id)) ?? null)
const noteParts = computed(() => t('sessionFlags.settings.note', { link: LINK_MARK }).split(LINK_MARK))

// Editor
const editor = ref<{ flag: SessionFlag | null } | null>(null)

// Reorder
const list = ref<HTMLElement | null>(null)
const drag = useDragSort({ container: list, onDrop: (from, to) => void commitOrder(moveId(flags.value.map((f) => f.id), from, to)) })
const lifted = ref<string | null>(null)

async function commitOrder(ids: string[]) {
  localIds.value = ids
  try {
    await store.reorder(null, ids)
  } catch (e) {
    toast.error(String(e))
  } finally {
    localIds.value = null
  }
}

function onHandleKey(e: KeyboardEvent, flag: SessionFlag) {
  const ids = flags.value.map((f) => f.id)
  if (e.key === ' ' || e.key === 'Enter') {
    e.preventDefault()
    if (lifted.value !== flag.id) {
      lifted.value = flag.id
      localIds.value = ids
    } else {
      lifted.value = null
      const next = localIds.value ?? ids
      if (next.join() !== store.flagsOf(null).map((f) => f.id).join()) void commitOrder(next)
      else localIds.value = null
    }
  } else if (lifted.value === flag.id && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
    e.preventDefault()
    const from = ids.indexOf(flag.id)
    localIds.value = moveId(ids, from, keyboardMove(ids, from, e.key === 'ArrowUp' ? -1 : 1))
  } else if (lifted.value === flag.id && e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    cancelLift()
  }
}

function cancelLift() {
  if (!lifted.value) return
  lifted.value = null
  localIds.value = null
}

async function setColor(flag: SessionFlag, color: ProjectColor) {
  if (color === flag.color) return
  try {
    await store.update(flag, { color })
  } catch (e) {
    toast.error(String(e))
  }
}

async function removeFlag(flag: SessionFlag) {
  const name = flagLabel(flag)
  const ok = await confirm({
    title: t('sessionFlags.settings.deleteTitle', { name }),
    message: t(flag.run_count > 0 ? 'sessionFlags.settings.deleteMessage' : 'sessionFlags.settings.deleteMessageNone', { n: flag.run_count }),
    confirmLabel: t('sessionFlags.settings.deleteConfirm'),
  })
  if (!ok) return
  try {
    const cleared = await store.remove(flag)
    toast.success(t(cleared > 0 ? 'sessionFlags.settings.deleted' : 'sessionFlags.settings.deletedNone', { name, n: cleared }))
  } catch (e) {
    toast.error(String(e))
  }
}

async function restoreDefaults() {
  try {
    const restored = await store.restoreDefaults()
    toast.success(t('sessionFlags.settings.restored', { n: restored.length }))
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <div class="space-y-4">
    <SettingsSectionHead :title="t('settings.sections.flags')" :intro="t('sessionFlags.settings.description')" />

    <div v-if="!loaded && !loadError" class="space-y-4">
      <div class="h-28 animate-pulse rounded-lg border border-border bg-card" />
    </div>

    <Banner v-else-if="loadError" tone="destructive" live>
      <p class="font-medium">{{ t('sessionFlags.settings.loadError') }}</p>
      <p class="break-words text-2xs opacity-80">{{ loadError }}</p>
      <template #actions>
        <Button variant="outline" size="sm" @click="load">{{ t('sessionFlags.settings.retry') }}</Button>
      </template>
    </Banner>

    <Card v-else class="overflow-visible" data-settings-id="flags-list">
      <template #header>
        <Flag class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('sessionFlags.settings.title') }}</span>
        <InfoTip :text="t('sessionFlags.settings.footer')" />
        <span class="ml-auto flex shrink-0 items-center gap-1.5">
          <Button variant="ghost" size="sm" :disabled="noRestore" @click="restoreDefaults">
            <RotateCcw class="h-3.5 w-3.5" :stroke-width="2" />
            {{ t('sessionFlags.settings.restore') }}
          </Button>
          <InfoTip :text="t('sessionFlags.settings.restoreHint')" />
          <Button variant="outline" size="sm" @click="editor = { flag: null }">
            <Plus class="h-3.5 w-3.5" :stroke-width="2" />
            {{ t('sessionFlags.settings.add') }}
          </Button>
        </span>
      </template>

      <p class="border-b border-border/50 px-4 py-2.5 text-xs text-muted-foreground">
        {{ noteParts[0] }}<button
          v-if="canOpenProjectFlags && lastProjectId"
          type="button"
          class="cursor-pointer text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          @click="openProjectFlags(lastProjectId)"
        >{{ t('sessionFlags.settings.projectLink') }}</button><template v-else>{{ t('sessionFlags.settings.projectLink') }}</template>{{ noteParts[1] }}
      </p>

      <div v-if="!flags.length" class="px-4 py-8 text-center">
        <p class="text-sm font-medium text-foreground">{{ t('sessionFlags.settings.emptyTitle') }}</p>
        <p class="mx-auto mt-1 max-w-md text-xs text-muted-foreground">{{ t('sessionFlags.settings.emptyBody') }}</p>
        <div class="mt-3 flex justify-center gap-2">
          <Button variant="outline" size="sm" @click="restoreDefaults">
            <RotateCcw class="h-3.5 w-3.5" :stroke-width="2" />
            {{ t('sessionFlags.settings.restore') }}
          </Button>
          <Button size="sm" @click="editor = { flag: null }">
            <Plus class="h-3.5 w-3.5" :stroke-width="2" />
            {{ t('sessionFlags.settings.add') }}
          </Button>
        </div>
      </div>

      <div v-else ref="list" class="relative">
        <FlagRow
          v-for="(flag, i) in flags"
          :key="flag.id"
          :flag="flag"
          :index="i"
          :flags="flags"
          :dragging="drag.dragFrom.value === i"
          :lifted="lifted === flag.id"
          :style="drag.itemStyle(i)"
          @edit="editor = { flag }"
          @remove="removeFlag(flag)"
          @color="setColor(flag, $event)"
          @handle-pointerdown="drag.start(i, $event)"
          @handle-keydown="onHandleKey($event, flag)"
          @handle-blur="cancelLift"
        />
        <div
          v-if="drag.isDragging.value && drag.dragTo.value !== drag.dragFrom.value"
          class="pointer-events-none absolute inset-x-0 z-20 h-0.5 rounded bg-primary"
          :style="{ top: `${(drag.dragTo.value ?? 0) * ROW_PX}px` }"
        />
      </div>

    </Card>

    <FlagEditorModal
      :open="!!editor"
      :flag="editor?.flag ?? null"
      :flags="flags"
      @close="editor = null"
    />
  </div>
</template>
