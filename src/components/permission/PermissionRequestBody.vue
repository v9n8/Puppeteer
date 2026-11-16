<script setup lang="ts">
// Body of a tool-permission request: Purpose → Impact → Tool + risk badges →
// command / diff / URL → full input. Shared by the permission window and the
// in-conversation prompt (V3-32). Questions and plans don't use it.
import { computed, onBeforeUnmount, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronsUpDown, Copy, Check } from 'lucide-vue-next'
import { classifyPermissionRisk, permissionDiffParts, type ImpactRequest } from '@/lib/permissionImpact'
import PermissionImpact from './PermissionImpact.vue'
import PermissionRiskBadges from './PermissionRiskBadges.vue'

const props = defineProps<{
  request: ImpactRequest & { display_name?: string | null }
  purpose?: string | null
  /** Open the Impact block from the start. */
  expanded?: boolean
}>()
const { t } = useI18n()

const MAX_LINES = 12

function asObject(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {}
}
const input = computed(() => asObject(props.request.tool_input))

const filePath = computed(() => {
  const p = input.value['file_path'] ?? input.value['path']
  return typeof p === 'string' ? p : ''
})
const diffParts = computed(() => permissionDiffParts(props.request))
const hasDiff = computed(() => diffParts.value.length > 0)
const risks = computed(() => classifyPermissionRisk(props.request))

const commandPreview = computed(() => {
  for (const k of ['command', 'url', 'pattern', 'query']) {
    if (typeof input.value[k] === 'string') return input.value[k] as string
  }
  return ''
})
const commandLines = computed(() => commandPreview.value.split('\n'))
const showAll = ref(false)
const shownCommand = computed(() =>
  showAll.value || commandLines.value.length <= MAX_LINES
    ? commandPreview.value
    : commandLines.value.slice(0, MAX_LINES).join('\n'),
)

const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | null = null
async function copyCommand() {
  try {
    await navigator.clipboard.writeText(commandPreview.value)
  } catch {
    return
  }
  copied.value = true
  if (copiedTimer) clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => (copied.value = false), 1500)
}
onBeforeUnmount(() => {
  if (copiedTimer) clearTimeout(copiedTimer)
})

const inputPreview = computed(() => {
  const inp = props.request.tool_input
  if (inp == null) return ''
  if (typeof inp === 'string') return inp
  try {
    return JSON.stringify(inp, null, 2)
  } catch {
    return String(inp)
  }
})

function splitLines(s: string): string[] {
  return s.length ? s.split('\n') : []
}
</script>

<template>
  <div class="space-y-3">
    <p v-if="purpose" class="text-sm leading-snug text-foreground">
      <span class="text-foreground/50">{{ t('permission.purpose') }} ·</span> {{ purpose }}
    </p>

    <PermissionImpact :request="request" :expanded="expanded" />

    <div class="flex flex-wrap items-center gap-2">
      <span class="text-2xs uppercase tracking-wider text-foreground/40">{{ t('permission.tool') }}</span>
      <span class="font-mono text-sm text-primary">{{ request.display_name || request.tool_name }}</span>
      <PermissionRiskBadges :risks="risks" />
      <span v-if="filePath" class="min-w-0 basis-full truncate font-mono text-xs text-foreground/50" :title="filePath">{{ filePath }}</span>
    </div>

    <!-- Diff view for Edit / MultiEdit / Write -->
    <div v-if="hasDiff" class="space-y-3">
      <div
        v-for="(part, i) in diffParts"
        :key="i"
        class="overflow-hidden rounded-md border border-border font-mono text-2xs leading-relaxed"
      >
        <div v-if="part.before" class="bg-red-500/10 dark:bg-red-500/5">
          <div v-for="(ln, li) in splitLines(part.before)" :key="'b' + li" class="flex">
            <span class="w-5 shrink-0 select-none bg-red-500/15 text-center text-red-600 dark:bg-red-500/10 dark:text-red-400/70">-</span>
            <span class="whitespace-pre-wrap break-words px-2 text-red-700 dark:text-red-200/90">{{ ln || ' ' }}</span>
          </div>
        </div>
        <div v-if="part.after" class="bg-emerald-500/10 dark:bg-emerald-500/5">
          <div v-for="(ln, li) in splitLines(part.after)" :key="'a' + li" class="flex">
            <span class="w-5 shrink-0 select-none bg-emerald-500/15 text-center text-emerald-600 dark:bg-emerald-500/10 dark:text-emerald-400/70">+</span>
            <span class="whitespace-pre-wrap break-words px-2 text-emerald-700 dark:text-emerald-100/90">{{ ln || ' ' }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Command / target for bash, web, etc. -->
    <div v-else-if="commandPreview" class="rounded-md border border-border bg-foreground/5 px-3 py-2">
      <div class="mb-1 flex items-center gap-2">
        <span class="text-2xs uppercase tracking-wider text-foreground/40">{{ t('permission.commandTarget') }}</span>
        <button
          type="button"
          class="ml-auto inline-flex h-7 cursor-pointer items-center gap-1 rounded px-2 text-2xs text-foreground/60 hover:bg-accent hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          @click="copyCommand"
        >
          <component :is="copied ? Check : Copy" class="h-3 w-3" :stroke-width="1.75" />
          {{ copied ? t('common.copied') : t('permission.copy') }}
        </button>
      </div>
      <pre class="whitespace-pre-wrap break-words font-mono text-xs text-foreground">{{ shownCommand }}</pre>
      <button
        v-if="commandLines.length > MAX_LINES && !showAll"
        type="button"
        class="mt-1 inline-flex h-7 cursor-pointer items-center gap-1 rounded px-2 text-2xs text-foreground/60 hover:bg-accent hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        @click="showAll = true"
      >
        <ChevronsUpDown class="h-3 w-3" :stroke-width="1.75" />
        {{ t('permission.showAllLines', { n: commandLines.length }) }}
      </button>
    </div>

    <details class="rounded-md border border-border bg-foreground/5">
      <summary class="min-h-7 cursor-pointer select-none px-3 py-2 text-xs text-foreground/60 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring">{{ t('permission.fullInput') }}</summary>
      <pre class="max-h-80 overflow-auto whitespace-pre-wrap break-words px-3 pb-2 font-mono text-2xs text-foreground/70">{{ inputPreview }}</pre>
    </details>
  </div>
</template>
