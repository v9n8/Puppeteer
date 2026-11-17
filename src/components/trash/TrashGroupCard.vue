<script setup lang="ts">
// One Trash block: a project with its trashed sessions. The block's buttons act
// on the whole block, or only on the sessions a search matched (`targets`).
// It only emits; TrashView runs the actions.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArchiveRestore, ChevronRight, FolderX, Trash2 } from 'lucide-vue-next'
import { Badge, Button } from '@/components/ui'
import { formatRunTimestamp, runStatusColorClass, runStatusIcon, shortModelId } from '@/lib/runDisplay'
import TrashSnippets from '@/components/trash/TrashSnippets.vue'
import { groupActionTargets, highlightParts, type TrashContentHit, type TrashEntry, type TrashRun, type TrashSession } from '@/lib/trash'

const props = defineProps<{
  entry: TrashEntry
  query: string
  open: boolean
  remote: boolean
  busy: boolean
  labelOf: (run: TrashRun) => string
  /** Content-search result per session row (by run id), shown under the row. */
  contentHits?: ReadonlyMap<string, TrashContentHit>
  /** The text the content hits are highlighted for. */
  contentQuery?: string
}>()
const emit = defineEmits<{
  toggle: []
  openProject: []
  restoreGroup: []
  purgeGroup: []
  restoreSession: [session: TrashSession]
  purgeSession: [session: TrashSession]
}>()

const { t } = useI18n()

const group = computed(() => props.entry.group)
const targets = computed(() => groupActionTargets(props.entry.full, props.entry.matched))
const shownCount = computed(() => group.value.sessions.length)

const restoreLabel = computed(() => {
  if (targets.value.partial) return t('trash.restoreMatched', { count: targets.value.sessions.length }, targets.value.sessions.length)
  return group.value.projectTrashed ? t('trash.restoreProject') : t('trash.restoreAll')
})
const purgeLabel = computed(() => {
  if (targets.value.partial) return t('trash.purgeMatched', { count: targets.value.sessions.length }, targets.value.sessions.length)
  return group.value.projectTrashed ? t('trash.purgeProject') : t('trash.purgeAll')
})

/** Sessions whose "+N worker" list is open. */
const workersOpen = ref(new Set<string>())
function toggleWorkers(id: string) {
  const next = new Set(workersOpen.value)
  if (!next.delete(id)) next.add(id)
  workersOpen.value = next
}
/** Workers a purge deletes that are not listed (not in the Trash themselves). */
function unlistedWorkers(s: TrashSession): number {
  return Math.max(0, s.run.worker_count - s.workers.length)
}
</script>

<template>
  <section class="rounded-card border border-border bg-card">
    <header class="flex items-center gap-2 px-3 py-2" :class="open && 'border-b border-border/60'">
      <button
        type="button"
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        :aria-expanded="open"
        :title="open ? t('trash.collapse') : t('trash.expand')"
        :aria-label="open ? t('trash.collapse') : t('trash.expand')"
        @click="emit('toggle')"
      >
        <ChevronRight class="h-4 w-4 transition-transform" :class="open && 'rotate-90'" :stroke-width="2" />
      </button>
      <FolderX v-if="group.projectTrashed" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
      <div class="min-w-0 flex-1">
        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            class="min-h-7 truncate text-md font-semibold"
            :class="group.projectTrashed ? 'cursor-default' : 'hover:underline'"
            :disabled="group.projectTrashed"
            @click="emit('openProject')"
          >
            <template v-for="(part, i) in highlightParts(group.project.name, entry.matched === null ? query : '')" :key="i">
              <mark v-if="part.hit" class="rounded bg-warning/30 text-inherit">{{ part.text }}</mark>
              <template v-else>{{ part.text }}</template>
            </template>
          </button>
          <Badge v-if="group.projectTrashed" tone="warning" size="xs">{{ t('trash.projectDeleted') }}</Badge>
          <span class="text-xs text-muted-foreground">{{ t('trash.sessionCount', { count: shownCount }, shownCount) }}</span>
        </div>
        <p class="truncate text-xs text-muted-foreground" :title="group.project.path">
          {{ group.project.path }}<template v-if="group.project.deleted_at"> · {{ t('trash.deletedAt', { at: formatRunTimestamp(group.project.deleted_at) }) }}</template>
        </p>
      </div>
      <Button variant="ghost" size="xs" class="min-h-7" :disabled="busy" @click="emit('restoreGroup')">
        <ArchiveRestore class="h-3 w-3" :stroke-width="2" />
        {{ restoreLabel }}
      </Button>
      <Button v-if="!remote" variant="destructive-ghost" size="xs" class="min-h-7" :disabled="busy" @click="emit('purgeGroup')">
        <Trash2 class="h-3 w-3" :stroke-width="2" />
        {{ purgeLabel }}
      </Button>
    </header>

    <template v-if="open">
      <p v-if="group.sessions.length === 0" class="px-3 py-2.5 text-xs text-muted-foreground">
        {{ t('trash.noSessions') }}
      </p>
      <div v-else class="divide-y divide-border/40">
        <div v-for="s in group.sessions" :key="s.run.id">
          <div class="flex items-center gap-3 px-3 py-2 text-[13px]">
            <component
              :is="runStatusIcon(s.run.status)"
              class="h-3.5 w-3.5 shrink-0"
              :class="runStatusColorClass(s.run.status)"
              :stroke-width="1.75"
            />
            <div class="min-w-0 flex-1 space-y-0.5">
              <div class="flex flex-wrap items-center gap-2">
                <span class="truncate font-medium">
                  <template v-for="(part, i) in highlightParts(labelOf(s.run), entry.matched === null ? '' : query)" :key="i">
                    <mark v-if="part.hit" class="rounded bg-warning/30 text-inherit">{{ part.text }}</mark>
                    <template v-else>{{ part.text }}</template>
                  </template>
                </span>
                <Badge size="xs">{{ s.run.engine }}</Badge>
                <span v-if="s.run.model" class="text-xs text-muted-foreground" :title="s.run.model">{{ shortModelId(s.run.model) }}</span>
                <button
                  v-if="s.workers.length || s.run.worker_count"
                  type="button"
                  class="inline-flex min-h-7 items-center rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  :aria-expanded="workersOpen.has(s.run.id)"
                  :title="t('trash.workersTitle')"
                  @click="toggleWorkers(s.run.id)"
                >
                  <Badge size="xs" class="cursor-pointer">{{ t('trash.workers', { count: s.run.worker_count || s.workers.length }, s.run.worker_count || s.workers.length) }}</Badge>
                </button>
              </div>
              <p class="text-xs text-muted-foreground">
                {{ t('trash.deletedAt', { at: formatRunTimestamp(s.deletedAt) }) }} · {{ t('trash.createdAt', { at: formatRunTimestamp(s.run.created_at) }) }}
              </p>
              <TrashSnippets v-if="contentHits?.get(s.run.id)" :hit="contentHits.get(s.run.id)!" :query="contentQuery ?? ''" />
            </div>
            <Button
              variant="ghost"
              size="icon-sm"
              class="min-h-7 min-w-7"
              :disabled="busy"
              :title="t('trash.restore')"
              :aria-label="t('trash.restore')"
              @click="emit('restoreSession', s)"
            >
              <ArchiveRestore class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
            <Button
              v-if="!remote"
              variant="destructive-ghost"
              size="icon-sm"
              class="min-h-7 min-w-7"
              :disabled="busy"
              :title="t('trash.purge')"
              :aria-label="t('trash.purge')"
              @click="emit('purgeSession', s)"
            >
              <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
          </div>
          <ul v-if="workersOpen.has(s.run.id)" class="space-y-1 border-t border-border/40 bg-muted/30 py-2 pl-10 pr-3 text-[13px]">
            <li v-for="w in s.workers" :key="w.id" class="flex items-center gap-2">
              <component
                :is="runStatusIcon(w.status)"
                class="h-3.5 w-3.5 shrink-0"
                :class="runStatusColorClass(w.status)"
                :stroke-width="1.75"
              />
              <span class="truncate">{{ labelOf(w) }}</span>
              <Badge size="xs">{{ w.engine }}</Badge>
              <span class="text-xs text-muted-foreground max-[901px]:hidden">{{ t('trash.withConductor') }}</span>
            </li>
            <li v-if="unlistedWorkers(s) > 0" class="text-xs text-muted-foreground">
              {{ t('trash.unlistedWorkers', { count: unlistedWorkers(s) }, unlistedWorkers(s)) }}
            </li>
          </ul>
        </div>
      </div>
    </template>
  </section>
</template>
