<script setup lang="ts">
// Conflicts section of a project's Library tab (V3-47): each card shows both
// copies (path + last edit), opens a side-by-side diff, and resolves with
// "Overwrite with central" or "Keep local". "Check again" rescans the disk.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { GitMerge, CheckCircle2 } from 'lucide-vue-next'
import { Badge, Button, Card } from '@/components/ui'
import SyncDiffModal from '@/components/SyncDiffModal.vue'
import { useToast } from '@/composables/useToast'
import { conflictPlace, scanMessage, scanOutcome, sortConflicts, truncateMiddle } from '@/lib/syncConflicts'
import { useProjectsStore, type Project, type ProjectSyncConflict, type SyncConflictSides } from '@/stores/projects'

const props = defineProps<{ project: Project }>()

const { t } = useI18n()
const { toast } = useToast()
const store = useProjectsStore()

const cards = computed(() => sortConflicts(store.projectConflicts[props.project.id] ?? []))

const checking = ref(false)
/** Id of the conflict being resolved; locks that card's buttons and the modal's. */
const resolving = ref<string | null>(null)

async function checkAgain() {
  checking.value = true
  try {
    const report = await store.checkProjectConflicts(props.project.id)
    toast.success(scanMessage(scanOutcome(report), (key, params) => t(key, params ?? {})))
  } catch (e) {
    toast.error(String(e))
  } finally {
    checking.value = false
  }
}

async function resolve(c: ProjectSyncConflict, overwrite: boolean): Promise<boolean> {
  resolving.value = c.id
  try {
    if (c.kind === 'skill') await store.resolveConflict(c.id, overwrite)
    else await store.resolveRuleConflict(c.id, overwrite)
    // Scan again so the card goes only if the disk agrees (or the choice is remembered).
    await store.checkProjectConflicts(props.project.id)
    return true
  } catch (e) {
    toast.error(String(e))
    return false
  } finally {
    resolving.value = null
  }
}

// --- Diff modal ---
const diffOf = ref<ProjectSyncConflict | null>(null)
const sides = ref<SyncConflictSides | null>(null)
const loadingSides = ref(false)

async function openDiff(c: ProjectSyncConflict) {
  diffOf.value = c
  sides.value = null
  loadingSides.value = true
  try {
    const result = await store.readSyncConflictSides(c.kind, c.id)
    if (diffOf.value?.id === c.id) sides.value = result
  } catch (e) {
    toast.error(String(e))
    if (diffOf.value?.id === c.id) diffOf.value = null
  } finally {
    loadingSides.value = false
  }
}

async function resolveFromModal(overwrite: boolean) {
  const c = diffOf.value
  if (c && await resolve(c, overwrite)) diffOf.value = null
}

function sidePath(path: string, block: string | null): string {
  return block ? `${path} (${block})` : path
}
const modalLeft = computed(() => ({
  label: t('projectDetail.sideProject'),
  path: sides.value ? sidePath(sides.value.project.path, sides.value.project.block) : '',
  content: sides.value?.project.content ?? null,
  error: sides.value?.project.error ?? null,
}))
const modalRight = computed(() => ({
  label: t('projectDetail.sideCentral'),
  path: sides.value ? sidePath(sides.value.central.path, sides.value.central.block) : '',
  content: sides.value?.central.content ?? null,
  error: sides.value?.central.error ?? null,
}))

function mtimeText(ms: number | null): string {
  return ms === null ? t('projectDetail.unreadable') : t('projectDetail.modifiedAt', { time: new Date(ms).toLocaleString() })
}
</script>

<template>
  <div class="max-w-2xl space-y-3">
    <div class="flex items-center justify-end">
      <Button
        variant="outline"
        size="xs"
        class="min-h-7"
        :disabled="checking"
        :title="t('projectDetail.checkAgainTitle')"
        @click="checkAgain"
      >{{ checking ? t('projectDetail.checking') : t('projectDetail.checkAgain') }}</Button>
    </div>

    <!-- Nothing left to resolve -->
    <div v-if="cards.length === 0" class="flex flex-col items-center gap-1.5 rounded-lg border border-border/60 px-4 py-10 text-center">
      <CheckCircle2 class="h-5 w-5 text-success" :stroke-width="1.75" />
      <p class="text-sm font-medium">{{ t('projectDetail.noConflictsTitle') }}</p>
      <p class="text-xs text-muted-foreground">{{ t('projectDetail.noConflictsHint') }}</p>
    </div>

    <Card v-for="c in cards" :key="c.id" class="@container border-warning/20" bodyClass="p-4">
      <div class="mb-3 flex items-start gap-3">
        <GitMerge class="mt-0.5 h-4 w-4 shrink-0 text-warning" :stroke-width="1.75" />
        <div class="min-w-0">
          <div class="flex flex-wrap items-center gap-1.5">
            <p class="break-all font-mono text-sm font-medium">{{ c.name }}</p>
            <Badge tone="neutral" size="xs" class="uppercase tracking-wide">
              {{ t(c.kind === 'skill' ? 'projectDetail.conflictSkillBadge' : 'projectDetail.conflictRuleBadge', { path: conflictPlace(c) }) }}
            </Badge>
          </div>
          <p class="mt-0.5 text-xs text-muted-foreground">
            {{ t('projectDetail.localChangesDetected', { date: new Date(c.detected_at).toLocaleDateString() }) }}
          </p>
        </div>
      </div>

      <!-- The two copies; stacked when the card is narrow -->
      <div class="mb-3 space-y-1 text-xs">
        <div class="grid grid-cols-1 gap-x-3 gap-y-0.5 @md:grid-cols-[7.5rem_minmax(0,1fr)_auto]">
          <span class="font-medium">{{ t('projectDetail.sideProject') }}</span>
          <span class="min-w-0 break-all font-mono text-muted-foreground @md:break-normal" :title="c.project_path">{{ truncateMiddle(c.project_path) }}</span>
          <span class="text-muted-foreground">{{ mtimeText(c.project_mtime) }}</span>
        </div>
        <p class="select-none pl-0 text-muted-foreground @md:pl-[7.5rem]" aria-hidden="true">≠</p>
        <div class="grid grid-cols-1 gap-x-3 gap-y-0.5 @md:grid-cols-[7.5rem_minmax(0,1fr)_auto]">
          <span class="font-medium">{{ t('projectDetail.sideCentral') }}</span>
          <span class="min-w-0 break-all font-mono text-muted-foreground @md:break-normal" :title="c.central_path">{{ truncateMiddle(c.central_path) }}</span>
          <span class="text-muted-foreground">{{ mtimeText(c.central_mtime) }}</span>
        </div>
      </div>

      <div class="flex flex-wrap gap-2">
        <Button variant="outline" class="min-h-7" @click="openDiff(c)">{{ t('projectDetail.viewDiff') }}</Button>
        <Button class="min-h-7 flex-1" :disabled="resolving === c.id" @click="resolve(c, true)">
          {{ t('projectDetail.overwriteWithCentral') }}
        </Button>
        <Button variant="outline" class="min-h-7 flex-1" :disabled="resolving === c.id" @click="resolve(c, false)">
          {{ t('projectDetail.keepLocal') }}
        </Button>
      </div>
    </Card>

    <SyncDiffModal
      :open="diffOf !== null"
      :title="diffOf ? `${diffOf.name} · ${conflictPlace(diffOf)}` : ''"
      :left="modalLeft"
      :right="modalRight"
      :loading="loadingSides"
      @close="diffOf = null"
    >
      <template #footer>
        <Button :disabled="resolving !== null || loadingSides" @click="resolveFromModal(true)">{{ t('projectDetail.overwriteWithCentral') }}</Button>
        <Button variant="outline" :disabled="resolving !== null || loadingSides" @click="resolveFromModal(false)">{{ t('projectDetail.keepLocal') }}</Button>
      </template>
    </SyncDiffModal>
  </div>
</template>
