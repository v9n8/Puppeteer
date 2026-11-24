<script setup lang="ts">
// Reading Slack DM threads, on a Slack MCP server's detail (#9; `slack_threads::dm`):
// the one switch for every project (the `slack_dm_read` setting, shared by every
// Slack server, so each shows the same value). Turning it on checks each token
// for the DM scopes in the background: the result informs and never undoes the
// switch. Each project then opts in on its own, on its MCP tab.
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertCircle, AlertTriangle, CheckCircle2, Loader2, XCircle } from 'lucide-vue-next'
import { InfoTip, ToggleSwitch } from '@/components/ui'
import DetailSection from '@/components/listDetail/DetailSection.vue'
import { useToast } from '@/composables/useToast'
import { invoke } from '@/lib/tauri'

/** One token's answer — mirrors the Rust `DmScopeResult` (never the token). */
interface DmScopeResult {
  serverId: string
  serverName: string
  status: 'ok' | 'missing' | 'unknown' | 'token_error'
  missing: string[]
}

const { t } = useI18n()
const { toast } = useToast()

const on = ref(false)
const saving = ref(false)
const checking = ref(false)
const results = ref<DmScopeResult[] | null>(null)
const checkError = ref<string | null>(null)

const STATUS_ICON = { ok: CheckCircle2, missing: AlertTriangle, unknown: AlertCircle, token_error: XCircle } as const
const STATUS_TONE = {
  ok: 'text-success',
  missing: 'text-warning',
  unknown: 'text-muted-foreground',
  token_error: 'text-destructive',
} as const

onMounted(async () => {
  try {
    on.value = await invoke<boolean>('get_slack_dm_read')
  } catch (e) {
    console.warn('[slackDm] get_slack_dm_read failed', e)
  }
})

async function toggle() {
  const next = !on.value
  saving.value = true
  try {
    await invoke('set_slack_dm_read', { on: next })
    on.value = next
    toast.success(t('mcp.slackDm.saved'))
  } catch (e) {
    toast.error(String(e))
    return
  } finally {
    saving.value = false
  }
  results.value = null
  checkError.value = null
  if (next) void checkScopes()
}

async function checkScopes() {
  checking.value = true
  try {
    results.value = await invoke<DmScopeResult[]>('check_slack_dm_scopes')
  } catch (e) {
    checkError.value = t('mcp.slackDm.checkFailed', { error: String(e) })
  } finally {
    checking.value = false
  }
}

function statusText(r: DmScopeResult) {
  return r.status === 'missing'
    ? t('mcp.slackDm.status.missing', { scopes: r.missing.join(', ') })
    : t(`mcp.slackDm.status.${r.status}`)
}
</script>

<template>
  <DetailSection :title="t('mcp.slackDm.title')">
    <div class="flex items-center gap-3">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-1.5">
          <span class="text-xs font-medium">{{ t('mcp.slackDm.global') }}</span>
          <InfoTip :text="t('mcp.slackDm.globalHint')" />
        </div>
      </div>
      <ToggleSwitch
        :checked="on"
        :loading="saving"
        :label="t('mcp.slackDm.global')"
        @toggle="toggle"
      />
    </div>
    <p v-if="checking" class="flex items-center gap-1.5 text-2xs text-muted-foreground">
      <Loader2 class="h-3 w-3 animate-spin" :stroke-width="2" />
      {{ t('mcp.slackDm.checking') }}
    </p>
    <p v-else-if="checkError" class="text-2xs text-destructive">{{ checkError }}</p>
    <template v-else-if="results">
      <p v-if="!results.length" class="text-2xs text-muted-foreground">{{ t('mcp.slackDm.noTokens') }}</p>
      <ul v-else class="space-y-1">
        <li v-for="r in results" :key="r.serverId" class="flex items-start gap-1.5 text-2xs">
          <component :is="STATUS_ICON[r.status]" class="mt-px h-3 w-3 shrink-0" :class="STATUS_TONE[r.status]" :stroke-width="2" />
          <span class="min-w-0">
            <span class="font-mono font-medium">{{ r.serverName }}</span>
            <span class="text-muted-foreground"> — {{ statusText(r) }}</span>
          </span>
        </li>
      </ul>
    </template>
  </DetailSection>
</template>
