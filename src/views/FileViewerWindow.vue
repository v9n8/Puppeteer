<script setup lang="ts">
// Bare, chrome-less host for the FileViewer, shown in a standalone OS window
// (opened via openFileWindow / `index.html?fileWindow=1&…`). Lets a file be read
// side-by-side with the main app window. Clicking a file link inside opens yet
// another pop-out window, keeping the "one window per file" model. Quote sends a
// selection to the session that opened the file (lib/quoteAppend).
import { computed, ref, onBeforeUnmount, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Pin, PinOff } from 'lucide-vue-next'
import { FILE_DELETED_EVENT, FILE_MOVED_EVENT } from '@/lib/fileEvents'
import FileViewer from '@/components/FileViewer.vue'
import { Button, ConfirmModal, PromptModal, ToastHost } from '@/components/ui'
import { FILE_WINDOW_ORIGIN, openFileWindow, type FileWindowOrigin } from '@/lib/fileWindow'
import { parseFileWindowQuery } from '@/lib/fileWindowCore'
import { pickQuoteRun, type QuoteSource } from '@/lib/quote'
import { sendQuote } from '@/lib/quoteAppend'
import { invoke } from '@/lib/tauri'
import { useFloatingWindow } from '@/composables/useFloatingWindow'
import { useProjectsStore } from '@/stores/projects'
import { runActivityAt, type RunRecord } from '@/stores/runs'

const { t } = useI18n()
// Floats above other apps while focused; pin to keep it on top deliberately.
const { pinned, togglePin } = useFloatingWindow()

const params = parseFileWindowQuery(window.location.search)
const projectPath = ref(params.projectPath)
const path = ref(params.path)
const line = ref<number | null>(params.line)
const originRunId = ref<string | null>(params.originRunId)

function setTitle(p: string) {
  document.title = (p.split('/').pop() || p) + ' — Puppeteer'
}

/** True when `candidate` IS our file, or the folder our file lives in. */
function covers(candidate: string): boolean {
  return path.value === candidate || path.value.startsWith(`${candidate}/`)
}

// ── Quote target ────────────────────────────────────────────────────────────
// The run that opened the file, else the project's latest session; none = no
// Quote. Read from the backend: this webview has no runs loaded, and the main
// window's runs store holds only the project on its screen.
const projects = useProjectsStore()
const quoteRun = ref<{ id: string; label: string } | null>(null)
let quoteRunSeq = 0

// Same naming as RunView's session list.
function runLabel(run: RunRecord): string {
  if (run.title?.trim()) return run.title.trim()
  if (run.run_type === 'session') return t('run.labelSession')
  return `${run.run_type === 'analyze_issue' ? t('run.labelIssue') : t('run.labelPr')} #${run.ref_number}`
}

async function resolveQuoteRun() {
  const seq = ++quoteRunSeq
  let next: { id: string; label: string } | null = null
  try {
    if (!projects.projects.length) await projects.fetchProjects()
    const project = projects.projects.find((p) => p.path === projectPath.value)
    if (project) {
      const runs = await invoke<RunRecord[]>('list_runs', { projectId: project.id })
      const run = pickQuoteRun(runs, originRunId.value, runActivityAt)
      if (run) next = { id: run.id, label: runLabel(run) }
    }
  } catch {
    next = null
  }
  if (seq === quoteRunSeq) quoteRun.value = next
}

const quoteTarget = computed(() => {
  const run = quoteRun.value
  if (!run) return null
  return {
    label: run.label,
    append: (text: string, source: QuoteSource, note?: string) => {
      sendQuote(run.id, text, source, note).catch(() => { /* event bus unavailable */ })
    },
  }
})

let unlisten: UnlistenFn[] = []

onMounted(async () => {
  setTitle(path.value)
  void resolveQuoteRun()

  // This window shows ONE path and has no other way to learn that the path
  // stopped being valid — the explorer that deleted or renamed it is a different
  // webview. So both are announced app-wide and acted on here.
  try {
    unlisten.push(
      await listen<{ projectPath: string; path: string }>(FILE_DELETED_EVENT, (e) => {
        const payload = e.payload
        if (!payload || payload.projectPath !== projectPath.value) return
        // Deleting the folder takes everything under it, this file included.
        if (covers(payload.path)) closeWindow()
      }),
      await listen<{ projectPath: string; from: string; to: string }>(FILE_MOVED_EVENT, (e) => {
        const payload = e.payload
        if (!payload || payload.projectPath !== projectPath.value) return
        if (path.value === payload.from) onRenamed(payload.to)
        else if (path.value.startsWith(`${payload.from}/`)) {
          onRenamed(payload.to + path.value.slice(payload.from.length))
        }
      }),
      // Opened again from another session: Quote follows it.
      await listen<FileWindowOrigin>(FILE_WINDOW_ORIGIN, (e) => {
        if (e.payload?.label !== getCurrentWindow().label) return
        originRunId.value = e.payload.originRunId
        void resolveQuoteRun()
      }),
      // Sessions come, go and get renamed while this window sits open.
      await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        if (focused) void resolveQuoteRun()
      }),
    )
  } catch {
    /* running outside the Tauri shell */
  }
})

onBeforeUnmount(() => {
  unlisten.forEach((off) => off())
  unlisten = []
})

function onOpenFile(p: string, l: number | null) {
  openFileWindow(projectPath.value, p, l, originRunId.value)
}
function onOpenUrl(url: string) {
  openUrl(url).catch(() => { /* opener unavailable */ })
}

// Renamed or moved (here or anywhere else): follow the file rather than sit on
// a path that no longer exists.
function onRenamed(next: string) {
  path.value = next
  line.value = null
  setTitle(next)
}

// Deleted (here or anywhere else): there is nothing left to show.
function closeWindow() {
  getCurrentWindow().close().catch(() => { /* already closing */ })
}
</script>

<template>
  <div class="h-screen w-screen bg-background text-foreground overflow-hidden">
    <FileViewer
      :project-path="projectPath"
      :path="path"
      :line="line"
      :quote-target="quoteTarget"
      @open-file="onOpenFile"
      @open-url="onOpenUrl"
      @renamed="onRenamed"
      @deleted="closeWindow"
    >
      <!-- Pin: keep this viewer above other apps even after it loses focus. -->
      <template #actions>
        <Button
          variant="ghost"
          size="icon-sm"
          class="shrink-0"
          :class="pinned && 'bg-primary/15 text-primary hover:bg-primary/25 hover:text-primary'"
          :title="pinned ? t('files.viewer.unpin') : t('files.viewer.pin')"
          :aria-label="pinned ? t('files.viewer.unpin') : t('files.viewer.pin')"
          :aria-pressed="pinned"
          @click="togglePin"
        >
          <component :is="pinned ? Pin : PinOff" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
      </template>
    </FileViewer>

    <!-- The ⋯ menu's delete asks, its rename prompts, and both report back —
         none of which works without these hosts, and this window doesn't mount
         the main app's. -->
    <ConfirmModal />
    <PromptModal />
    <ToastHost />
  </div>
</template>
