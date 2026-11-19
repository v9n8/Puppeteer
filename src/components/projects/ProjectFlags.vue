<script setup lang="ts">
// Project › Flag (V3-21): which shared flags this project's Sessions can use (switch per
// flag) and the project's own flags (add, edit, delete, reorder by pointer or keyboard).
// Writes go through the sessionFlags store, which reloads the project's list and tells
// the other windows; it reloads here too when Settings changes the shared flags.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Flag, Plus, Settings } from 'lucide-vue-next'
import { Banner, Button, Card } from '@/components/ui'
import ProjectSharedFlagRow from '@/components/projects/ProjectSharedFlagRow.vue'
import ProjectOwnFlagRow from '@/components/projects/ProjectOwnFlagRow.vue'
import FlagEditorModal from '@/components/flags/FlagEditorModal.vue'
import { useFlagTargets } from '@/components/sessions/useFlagTargets'
import { useDragSort } from '@/composables/useDragSort'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import type { Project } from '@/stores/projects'
import { keyboardMove, moveId } from '@/lib/flagEditor'
import { flagLabel, type SessionFlag } from '@/lib/sessionFlags'

const ROW_PX = 44

const props = defineProps<{ project: Project }>()

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const store = useSessionFlagsStore()
const { canManage, openManage } = useFlagTargets()

const loaded = ref(false)
const loadError = ref<string | null>(null)

async function load() {
  loadError.value = null
  loaded.value = false
  try {
    await store.load(props.project.id)
    loaded.value = true
  } catch (e) {
    loadError.value = String(e)
  }
}
onMounted(load)
watch(() => props.project.id, load)

const all = computed(() => store.flagsOf(props.project.id))
const shared = computed(() => all.value.filter((f) => f.project_id === null))
const storedOwn = computed(() => all.value.filter((f) => f.project_id !== null))

// The order on screen: the store's, unless a move is in flight (kept here until it is saved).
const localIds = ref<string[] | null>(null)
const own = computed<SessionFlag[]>(() => {
  const ids = localIds.value
  if (!ids) return storedOwn.value
  const byId = new Map(storedOwn.value.map((f) => [f.id, f]))
  return ids.map((id) => byId.get(id)).filter((f): f is SessionFlag => !!f)
})

// Editor: `flag` null adds one
const editor = ref<{ flag: SessionFlag | null } | null>(null)

// Reorder
const list = ref<HTMLElement | null>(null)
const drag = useDragSort({ container: list, onDrop: (from, to) => void commitOrder(moveId(own.value.map((f) => f.id), from, to)) })
const lifted = ref<string | null>(null)

async function commitOrder(ids: string[]) {
  localIds.value = ids
  try {
    await store.reorder(props.project.id, ids)
  } catch (e) {
    toast.error(String(e))
  } finally {
    localIds.value = null
  }
}

function onHandleKey(e: KeyboardEvent, flag: SessionFlag) {
  const ids = own.value.map((f) => f.id)
  if (e.key === ' ' || e.key === 'Enter') {
    e.preventDefault()
    if (lifted.value !== flag.id) {
      lifted.value = flag.id
      localIds.value = ids
    } else {
      lifted.value = null
      const next = localIds.value ?? ids
      if (next.join() !== storedOwn.value.map((f) => f.id).join()) void commitOrder(next)
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

async function removeFlag(flag: SessionFlag) {
  const name = flagLabel(flag, t)
  const ok = await confirm({
    title: t('sessionFlags.project.deleteTitle', { name }),
    message: t(flag.run_count > 0 ? 'sessionFlags.project.deleteMessage' : 'sessionFlags.project.deleteMessageNone', { n: flag.run_count }),
    confirmLabel: t('sessionFlags.project.deleteConfirm'),
  })
  if (!ok) return
  try {
    await store.remove(flag)
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <div class="max-w-2xl space-y-4">
    <div v-if="!loaded && !loadError" class="h-28 animate-pulse rounded-lg border border-border bg-card" />

    <Banner v-else-if="loadError" tone="destructive" live>
      <p class="font-medium">{{ t('sessionFlags.project.loadError') }}</p>
      <p class="break-words text-2xs opacity-80">{{ loadError }}</p>
      <template #actions>
        <Button variant="outline" size="sm" @click="load">{{ t('sessionFlags.project.retry') }}</Button>
      </template>
    </Banner>

    <template v-else>
      <Card class="overflow-visible">
        <template #header>
          <Flag class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
          <span class="text-xs font-semibold">{{ t('sessionFlags.project.sharedTitle') }}</span>
          <span class="text-xs tabular-nums text-muted-foreground">{{ shared.length }}</span>
          <span v-if="canManage" class="ml-auto flex shrink-0 items-center">
            <Button variant="ghost" size="icon-sm" :title="t('sessionFlags.project.sharedSettingsTip')" :aria-label="t('sessionFlags.project.sharedSettingsTip')" @click="openManage">
              <Settings class="h-3.5 w-3.5" :stroke-width="1.5" />
            </Button>
          </span>
        </template>
        <p v-if="!shared.length" class="px-4 py-6 text-center text-sm text-muted-foreground">{{ t('sessionFlags.project.sharedEmpty') }}</p>
        <ProjectSharedFlagRow
          v-for="flag in shared"
          :key="flag.id"
          :flag="flag"
          :project-id="project.id"
          :project-name="project.name"
        />
      </Card>

      <Card class="overflow-visible">
        <template #header>
          <Flag class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
          <span class="text-xs font-semibold">{{ t('sessionFlags.project.ownTitle') }}</span>
          <span class="ml-auto flex shrink-0 items-center">
            <Button variant="outline" size="sm" :title="t('sessionFlags.project.addTip')" @click="editor = { flag: null }">
              <Plus class="h-3.5 w-3.5" :stroke-width="2" />
              {{ t('sessionFlags.project.add') }}
            </Button>
          </span>
        </template>
        <p v-if="!own.length" class="px-4 py-6 text-center text-sm text-muted-foreground">{{ t('sessionFlags.project.ownEmpty') }}</p>
        <div v-else ref="list" class="relative">
          <ProjectOwnFlagRow
            v-for="(flag, i) in own"
            :key="flag.id"
            :flag="flag"
            :index="i"
            :dragging="drag.dragFrom.value === i"
            :lifted="lifted === flag.id"
            :style="drag.itemStyle(i)"
            @edit="editor = { flag }"
            @remove="removeFlag(flag)"
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

      <p class="text-2xs text-muted-foreground">{{ t('sessionFlags.project.footer', { project: project.name }) }}</p>
    </template>

    <FlagEditorModal
      :open="!!editor"
      :flag="editor?.flag ?? null"
      :flags="storedOwn"
      :project-id="project.id"
      :project-name="project.name"
      @close="editor = null"
    />
  </div>
</template>
