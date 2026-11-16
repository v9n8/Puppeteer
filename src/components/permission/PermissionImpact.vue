<script setup lang="ts">
// "Impact" block of a permission request: folded, with a one-line summary as its
// title; opened, it lists what would change, where, and over the network. Pure
// guess from the tool name and input (see lib/permissionImpact.ts).
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronDown, ChevronRight } from 'lucide-vue-next'
import {
  permissionDiffParts,
  permissionImpact,
  type ImpactNetwork,
  type ImpactRequest,
} from '@/lib/permissionImpact'
import PermissionRiskBadges from './PermissionRiskBadges.vue'

const props = defineProps<{ request: ImpactRequest; expanded?: boolean }>()
const { t } = useI18n()

const open = ref(props.expanded ?? false)
const impact = computed(() => permissionImpact(props.request))
const summary = computed(() => t(`permission.impact.${impact.value.summary.key}`, impact.value.summary.params))

// A few lines of the first change; the full diff sits in the request body below.
const PREVIEW_LINES = 3
const preview = computed(() => {
  const part = permissionDiffParts(props.request)[0]
  if (!part) return null
  const cut = (s: string) => (s ? s.split('\n') : [])
  const before = cut(part.before)
  const after = cut(part.after)
  return {
    before: before.slice(0, PREVIEW_LINES),
    after: after.slice(0, PREVIEW_LINES),
    more: before.length > PREVIEW_LINES || after.length > PREVIEW_LINES,
  }
})

function networkLabel(n: ImpactNetwork): string {
  if (n.kind === 'git') return `${t('permission.impact.networkGit')} (git ${n.method ?? ''})`.replace(' )', ')')
  if (n.kind === 'package') return t('permission.impact.networkPackage')
  return n.host || t('permission.impact.unknown')
}

interface Row {
  label: string
  value?: string
  mono?: boolean
  badges?: boolean
}
const rows = computed<Row[]>(() => {
  const i = impact.value
  const out: Row[] = []
  if (i.mcpServer !== undefined) {
    out.push({ label: t('permission.impact.mcpServer'), value: i.mcpServer })
    if (i.mcpTool) out.push({ label: t('permission.impact.mcpTool'), value: i.mcpTool, mono: true })
  } else if (props.request.tool_name === 'WebFetch' || props.request.tool_name === 'WebSearch') {
    out.push({ label: t('permission.impact.method'), value: i.network[0]?.method ?? 'GET', mono: true })
    out.push({ label: t('permission.impact.domain'), value: i.network[0]?.host || t('permission.impact.unknown'), mono: true })
    out.push({ label: t('permission.impact.writesFiles'), value: t('permission.impact.no') })
  } else {
    if (i.cwd) out.push({ label: t('permission.impact.cwd'), value: i.cwd, mono: true })
    out.push({
      label: t('permission.impact.network'),
      value: i.network.length ? [...new Set(i.network.map(networkLabel))].join(', ') : t('permission.impact.no'),
    })
  }
  out.push({ label: t('permission.impact.risk'), badges: true })
  return out
})
</script>

<template>
  <div class="rounded-md border border-border bg-foreground/[0.03]">
    <button
      type="button"
      class="flex min-h-9 w-full cursor-pointer items-center gap-2 rounded-md px-3 text-left text-xs focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :aria-expanded="open"
      @click="open = !open"
    >
      <component :is="open ? ChevronDown : ChevronRight" class="h-3.5 w-3.5 shrink-0 text-foreground/50" :stroke-width="2" />
      <b class="shrink-0 font-medium text-foreground">{{ t('permission.impact.title') }}</b>
      <span class="min-w-0 flex-1 truncate text-foreground/60" :title="summary">{{ summary }}</span>
    </button>

    <div v-if="open" class="space-y-2 border-t border-border px-3 py-2.5 text-xs">
      <template v-if="impact.files.length">
        <p class="text-2xs uppercase tracking-wider text-foreground/40">{{ t('permission.impact.files') }}</p>
        <div v-for="f in impact.files" :key="f.path" class="flex items-center gap-2">
          <code class="min-w-0 flex-1 truncate font-mono text-foreground" :title="f.path">{{ f.path || t('permission.impact.unknown') }}</code>
          <template v-if="f.added || f.removed">
            <span class="shrink-0 font-mono text-success">+{{ f.added }}</span>
            <span class="shrink-0 font-mono text-destructive">−{{ f.removed }}</span>
          </template>
        </div>
        <div v-if="preview" class="overflow-hidden rounded border border-border font-mono text-2xs leading-relaxed">
          <div v-for="(ln, li) in preview.before" :key="'b' + li" class="flex bg-destructive/10">
            <span class="w-5 shrink-0 select-none text-center text-destructive">-</span>
            <span class="min-w-0 flex-1 truncate px-2 text-foreground/80">{{ ln || ' ' }}</span>
          </div>
          <div v-for="(ln, li) in preview.after" :key="'a' + li" class="flex bg-success/10">
            <span class="w-5 shrink-0 select-none text-center text-success">+</span>
            <span class="min-w-0 flex-1 truncate px-2 text-foreground/80">{{ ln || ' ' }}</span>
          </div>
          <div v-if="preview.more" class="px-2 text-foreground/40">…</div>
        </div>
      </template>

      <template v-if="impact.commands.length">
        <p class="text-2xs uppercase tracking-wider text-foreground/40">{{ t('permission.impact.commands') }}</p>
        <div v-for="(c, ci) in impact.commands" :key="ci" class="flex flex-wrap items-center gap-x-2 gap-y-1">
          <code class="min-w-0 flex-1 basis-40 truncate font-mono text-foreground" :title="c.cmd">{{ c.cmd }}</code>
          <span class="flex shrink-0 items-center gap-1"><PermissionRiskBadges :risks="c.risks" /></span>
        </div>
      </template>

      <dl class="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-3 gap-y-1.5">
        <template v-for="r in rows" :key="r.label">
          <dt class="text-foreground/50">{{ r.label }}</dt>
          <dd class="min-w-0" :class="r.mono ? 'truncate font-mono' : ''" :title="r.value">
            <span v-if="r.badges" class="flex flex-wrap items-center gap-1"><PermissionRiskBadges :risks="impact.risks" /></span>
            <template v-else>{{ r.value }}</template>
          </dd>
        </template>
      </dl>

      <p class="text-2xs text-foreground/40">{{ t('permission.impact.guess') }}</p>
    </div>
  </div>
</template>
