<script setup lang="ts">
// The "Changed files" tab of the Session area (V3-26): on the left the files this
// Session changed (edited or created, with +added / −removed over the whole
// Session), on the right the diff of the selected file, one block per edit. The
// numbers come from the whole log, not only the part the conversation loaded.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, FileDiff, RefreshCw, SquareArrowOutUpRight } from 'lucide-vue-next'
import DiffView from '@/components/DiffView.vue'
import { Button } from '@/components/ui'
import { useSessionTabsStore } from '@/stores/sessionTabs'
import { useChangedFiles } from './useChangedFiles'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const { currentRunId, project } = useRunSession()
const tabsStore = useSessionTabsStore()
const { files, loading, error, reload } = useChangedFiles()

const runId = computed(() => currentRunId.value ?? '')

/** Project-relative when the file is inside the Project (the Session may report absolute paths). */
function relPath(path: string): string {
  const root = (project.value?.path ?? '').replace(/[\\/]+$/, '')
  if (root && (path.startsWith(`${root}/`) || path.startsWith(`${root}\\`))) return path.slice(root.length + 1)
  return path
}

const selectedPath = ref<string | null>(null)
// First file selected by default; one that left the list (another Session) is replaced.
watch(files, (list) => {
  if (!list.some((f) => f.path === selectedPath.value)) selectedPath.value = list[0]?.path ?? null
}, { immediate: true })
const selected = computed(() => files.value.find((f) => f.path === selectedPath.value) ?? null)

function onListKey(e: KeyboardEvent) {
  if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
  e.preventDefault()
  const list = files.value
  if (!list.length) return
  const i = list.findIndex((f) => f.path === selectedPath.value)
  const next = Math.min(list.length - 1, Math.max(0, i + (e.key === 'ArrowDown' ? 1 : -1)))
  selectedPath.value = list[next].path
  const el = (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-path="${CSS.escape(list[next].path)}"]`)
  el?.scrollIntoView({ block: 'nearest' })
}

function openFile(path: string) {
  tabsStore.open(runId.value, relPath(path), null)
}
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col overflow-hidden bg-card">
    <div class="flex h-11 shrink-0 items-center gap-2 border-b border-border px-3">
      <FileDiff class="h-4 w-4 shrink-0 text-muted-foreground" :stroke-width="1.75" aria-hidden="true" />
      <span class="truncate text-sm font-medium text-foreground">{{ t('run.changedFiles.title', { n: files.length }) }}</span>
      <span
        class="truncate text-xs text-muted-foreground max-[901px]:hidden"
        :title="t('run.changedFiles.subtitleTip')"
      >{{ t('run.changedFiles.subtitle') }}</span>
      <Button
        variant="ghost"
        size="icon-sm"
        class="ml-auto shrink-0"
        :disabled="loading"
        :title="t('run.changedFiles.reload')"
        :aria-label="t('run.changedFiles.reload')"
        @click="reload"
      >
        <RefreshCw class="h-4 w-4" :class="{ 'animate-spin': loading }" :stroke-width="1.75" aria-hidden="true" />
      </Button>
    </div>

    <div v-if="error" role="alert" class="flex shrink-0 items-center gap-2 border-b border-border bg-destructive/10 px-3 py-2 text-xs text-destructive">
      <AlertTriangle class="h-4 w-4 shrink-0" :stroke-width="1.75" aria-hidden="true" />
      <span class="min-w-0 flex-1 truncate" :title="error">{{ t('run.changedFiles.error') }}</span>
      <Button variant="ghost" size="sm" class="shrink-0" @click="reload">{{ t('run.changedFiles.retry') }}</Button>
    </div>

    <div v-if="!files.length" class="flex flex-1 items-center justify-center p-6 text-sm text-muted-foreground">
      {{ loading ? t('common.loading') : t('run.changedFiles.empty') }}
    </div>

    <div v-else class="flex min-h-0 flex-1">
      <!-- Files: arrow keys move the selection -->
      <ul
        role="listbox"
        tabindex="0"
        :aria-label="t('run.changedFiles.title', { n: files.length })"
        class="w-60 shrink-0 overflow-y-auto border-r border-border py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring min-[1100px]:w-72"
        @keydown="onListKey"
      >
        <li v-for="f in files" :key="f.path" role="option" :aria-selected="f.path === selectedPath">
          <button
            type="button"
            tabindex="-1"
            :data-path="f.path"
            :title="f.path"
            class="flex min-h-8 w-full items-center gap-2 px-3 py-1 text-left text-xs hover:bg-accent"
            :class="f.path === selectedPath ? 'bg-accent text-foreground' : 'text-foreground/80'"
            @click="selectedPath = f.path"
          >
            <span
              class="min-w-8 shrink-0 whitespace-nowrap text-2xs"
              :class="f.kind === 'create' ? 'text-success' : 'text-warning'"
            >{{ t(f.kind === 'create' ? 'run.changedFiles.create' : 'run.changedFiles.edit') }}</span>
            <!-- rtl + isolate: the "…" cuts the start of a long path, and the end stays readable -->
            <span class="min-w-0 flex-1 truncate font-mono" style="direction: rtl; text-align: left"><bdi>{{ relPath(f.path) }}</bdi></span>
            <span class="shrink-0 font-mono text-2xs">
              <span v-if="f.unknown" class="text-muted-foreground">+?</span>
              <template v-else>
                <span v-if="f.added" class="text-emerald-600 dark:text-emerald-400/80">+{{ f.added }}</span>
                <span v-if="f.removed" class="ml-1 text-red-600 dark:text-red-400/80">−{{ f.removed }}</span>
              </template>
            </span>
          </button>
        </li>
      </ul>

      <!-- Diff of the selected file: one block per edit -->
      <section v-if="selected" class="min-w-0 flex-1 overflow-y-auto">
        <div class="sticky top-0 z-10 flex items-center gap-2 border-b border-border bg-card px-3 py-1.5">
          <span class="min-w-0 flex-1 truncate font-mono text-xs text-foreground" :title="selected.path">{{ relPath(selected.path) }}</span>
          <span class="shrink-0 font-mono text-2xs">
            <span v-if="selected.unknown" class="text-muted-foreground">+?</span>
            <template v-else>
              <span class="text-emerald-600 dark:text-emerald-400/80">+{{ selected.added }}</span>
              <span class="ml-1 text-red-600 dark:text-red-400/80">−{{ selected.removed }}</span>
            </template>
          </span>
          <Button variant="ghost" size="sm" class="shrink-0" @click="openFile(selected.path)">
            <SquareArrowOutUpRight class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
            {{ t('run.changedFiles.openFile') }}
          </Button>
        </div>
        <div class="space-y-3 p-3">
          <div v-for="(edit, i) in selected.edits" :key="i" class="space-y-1.5">
            <div class="text-2xs font-medium uppercase tracking-wider text-muted-foreground">{{ t('run.changedFiles.turn', { n: edit.turn }) }}</div>
            <p v-if="edit.tooLarge" class="rounded-md border border-border px-3 py-2 text-xs text-muted-foreground">
              {{ t('run.changedFiles.tooLarge') }}
            </p>
            <DiffView v-for="(part, pi) in edit.parts" :key="pi" :before="part.before" :after="part.after" />
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
