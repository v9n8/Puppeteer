<script setup lang="ts">
// Search across sessions: one box over the prompts and replies of every run of
// every project (mirrored CLI sessions included), from the index `search_sessions`
// keeps (sessions.cross_session_search). Hits are listed per run; picking one opens
// the run at that message, with the query highlighted (RunView reads it back).
//
// In App.vue's KeepAlive list, so the query and results are still there after
// opening a hit; re-entry runs the search again for anything indexed since.
import { computed, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { Search, X } from 'lucide-vue-next'
import { AppSelect, Banner, Button, Input, PageHeader } from '@/components/ui'
import { useActivatedRefresh } from '@/composables/useActivatedRefresh'
import { useProjectsStore } from '@/stores/projects'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import { formatRunTimestamp, relativeAge } from '@/lib/runDisplay'
import {
  CMD_SEARCH_SESSIONS,
  groupSessionHits,
  hitRouteQuery,
  snippetParts,
  type SessionHitGroup,
  type SessionSearch,
} from '@/lib/sessionSearch'

defineOptions({ name: 'SessionSearchView' })

const { t } = useI18n()
const router = useRouter()
const projects = useProjectsStore()

const query = ref('')
/** '' = every project. */
const projectFilter = ref('')
const projectOptions = computed(() => [
  { value: '', label: t('sessionSearch.allProjects') },
  ...projects.projects.map((p) => ({ value: p.id, label: p.name })),
])

const groups = shallowRef<SessionHitGroup[]>([])
const hitCount = ref(0)
const truncated = ref(false)
const searching = ref(false)
const error = ref<string | null>(null)
/** The query the results on screen answer, for the highlight and the links. */
const searched = ref('')

// The index answers fast, but each keystroke is still a query: wait for a pause.
const DEBOUNCE_MS = 250
let timer: ReturnType<typeof setTimeout> | null = null
// Bumped by every search, so a slower stale answer is dropped.
let seq = 0

async function runSearch() {
  const q = query.value.trim()
  const mine = ++seq
  if (!q) {
    groups.value = []
    hitCount.value = 0
    truncated.value = false
    searching.value = false
    error.value = null
    searched.value = ''
    return
  }
  searching.value = true
  try {
    const r = await invoke<SessionSearch>(CMD_SEARCH_SESSIONS, {
      query: q,
      projectId: projectFilter.value || null,
    })
    if (mine !== seq) return
    groups.value = groupSessionHits(r.hits)
    hitCount.value = r.hits.length
    truncated.value = r.truncated
    error.value = null
  } catch (e) {
    if (mine !== seq) return
    groups.value = []
    hitCount.value = 0
    truncated.value = false
    error.value = errorText(e, t)
  }
  searched.value = q
  searching.value = false
}

watch([query, projectFilter], () => {
  if (timer) clearTimeout(timer)
  timer = setTimeout(runSearch, DEBOUNCE_MS)
})

if (projects.projects.length === 0) projects.fetchProjects().catch(() => {})
useActivatedRefresh(() => {
  projects.fetchProjects().catch(() => {})
  if (query.value.trim()) return runSearch()
})

function runLabel(g: SessionHitGroup): string {
  if (g.title?.trim()) return g.title.trim()
  if (g.runType === 'session') return t('run.labelSession')
  return g.runType === 'analyze_issue' ? t('run.labelIssue') : t('run.labelPr')
}

function openHit(g: SessionHitGroup, offset: number) {
  router
    .push({
      name: 'project-run-detail',
      params: { projectId: g.projectId, runId: g.runId },
      query: hitRouteQuery({ offset, find: searched.value }),
    })
    .catch(() => {})
}
</script>

<template>
  <div class="flex flex-col h-full">
    <PageHeader :title="t('sessionSearch.title')" :count="searched ? groups.length : null" />

    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-4xl space-y-4">
        <div class="flex items-center gap-2">
          <div class="relative flex-1 min-w-0">
            <Search class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.75" />
            <Input
              v-model="query"
              autofocus
              :placeholder="t('sessionSearch.placeholder')"
              :aria-label="t('sessionSearch.placeholder')"
              class="pl-8 pr-8"
              @keydown.esc="query = ''"
            />
            <Button
              v-if="query"
              variant="ghost"
              size="icon-xs"
              class="absolute right-1 top-1/2 -translate-y-1/2"
              :title="t('run.clearSearch')"
              :aria-label="t('run.clearSearch')"
              @click="query = ''"
            >
              <X class="h-3.5 w-3.5" :stroke-width="2" />
            </Button>
          </div>
          <AppSelect v-model="projectFilter" :options="projectOptions" class="w-48 shrink-0" />
        </div>

        <Banner v-if="error" tone="destructive">{{ error }}</Banner>

        <p v-else-if="!searched" class="text-xs text-muted-foreground">
          {{ searching ? t('sessionSearch.searching') : t('sessionSearch.hint') }}
        </p>

        <p v-else-if="groups.length === 0" class="text-xs text-muted-foreground">
          {{ searching ? t('sessionSearch.searching') : t('sessionSearch.noMatches', { query: searched }) }}
        </p>

        <template v-else>
          <p v-if="truncated" class="text-xs text-muted-foreground">
            {{ t('sessionSearch.truncated', { count: hitCount }) }}
          </p>
          <div
            v-for="g in groups"
            :key="g.runId"
            class="rounded-lg border border-border bg-card"
            :class="searching && 'opacity-60'"
          >
            <div class="flex items-center gap-2 border-b border-border/50 px-3 py-2 text-xs">
              <span class="min-w-0 truncate font-semibold" :title="runLabel(g)">{{ runLabel(g) }}</span>
              <span class="shrink-0 text-2xs text-muted-foreground">({{ g.hits.length }})</span>
              <span class="ml-auto shrink-0 truncate text-2xs text-muted-foreground">{{ g.projectName }}</span>
              <span class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-2xs capitalize text-muted-foreground">{{ g.engine }}</span>
              <span class="shrink-0 text-2xs tabular-nums text-muted-foreground" :title="formatRunTimestamp(g.activityAt)">
                {{ relativeAge(g.activityAt, t('run.relNow')) }}
              </span>
            </div>
            <div class="py-1">
              <button
                v-for="hit in g.hits"
                :key="hit.offset"
                type="button"
                class="w-full px-3 py-1.5 text-left text-xs text-foreground/80 break-words line-clamp-2 transition-colors hover:bg-accent/60 hover:text-foreground cursor-pointer"
                :title="t('sessionSearch.openHit')"
                @click="openHit(g, hit.offset)"
              >
                <template v-for="(part, i) in snippetParts(hit.snippet, searched)" :key="i">
                  <mark v-if="part.match" class="rounded-sm bg-primary/25 text-foreground">{{ part.text }}</mark>
                  <template v-else>{{ part.text }}</template>
                </template>
              </button>
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>
