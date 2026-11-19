<script setup lang="ts">
// Settings → "Routing" → policy versions (QO-FR46 … QO-FR50): "Update now", the
// proposal a recompute left while automatic updates are off, and the version
// log — each version's moves with the counts behind them, its full text, and
// "Go back to this version" (asks first; switches automatic updates off).
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { History, RefreshCw } from 'lucide-vue-next'
import { Badge, Button, Card, InfoTip, Modal } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import {
  changeText, sourceLabelKey,
  type RoutingChange, type RoutingPolicies, type RoutingPolicyVersion, type RoutingRecomputed,
} from '@/lib/routing'

// The parent reloads the settings form: going back switched automatic updates off.
const emit = defineEmits<{ reverted: [] }>()

const { t } = useI18n()
const { toast } = useToast()
const { confirm } = useConfirm()

const policies = ref<RoutingPolicies | null>(null)
const busy = ref(false)
const viewing = ref<RoutingPolicyVersion | null>(null)

async function load() {
  try {
    policies.value = await invoke<RoutingPolicies>('list_routing_policies')
  } catch (e) {
    toast.error(errorText(e, t))
  }
}
onMounted(load)

async function updateNow() {
  busy.value = true
  try {
    const done = await invoke<RoutingRecomputed>('recompute_routing_policy')
    // A new version is announced by the app-wide notice (useRoutingNotice).
    if (done.result === 'unchanged') toast.info(t('routing.update.unchanged'))
    else if (done.result === 'proposed') toast.info(t('routing.update.proposed'))
    await load()
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    busy.value = false
  }
}

async function goBack(v: RoutingPolicyVersion) {
  const ok = await confirm({
    title: t('routing.versions.revertTitle', { n: v.version }),
    message: t('routing.versions.revertMessage', { n: v.version }),
    confirmLabel: t('routing.versions.revert'),
    variant: 'primary',
  })
  if (!ok) return
  busy.value = true
  try {
    const created = await invoke<number>('revert_routing_policy', { version: v.version })
    toast.success(t('routing.versions.reverted', { n: created, from: v.version }))
    emit('reverted')
    await load()
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    busy.value = false
  }
}

function sourceLabel(source: string): string {
  const key = sourceLabelKey(source)
  return key ? t(key) : source
}

function basisText(c: RoutingChange): string {
  return c.basis ? t('routing.versions.basis', { ...c.basis }) : ''
}
</script>

<template>
  <Card body-class="p-4 space-y-3">
    <template #header>
      <History class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('routing.versions.title') }}</span>
      <InfoTip :text="t('routing.versions.hint')" />
      <Button class="ml-auto" variant="outline" size="xs" :disabled="busy" data-settings-id="routing-updateNow" @click="updateNow">
        <RefreshCw class="h-3 w-3" :stroke-width="1.75" />
        {{ t('routing.update.now') }}
      </Button>
    </template>

    <div v-if="policies?.proposal?.changes.length" class="space-y-1.5 rounded-lg border border-warning/40 bg-warning/5 p-3">
      <p class="text-xs font-medium">{{ t('routing.proposal.title', { n: policies.proposal.based_on }) }}</p>
      <ul class="space-y-0.5 text-2xs">
        <li v-for="c in policies.proposal.changes" :key="c.kind">
          <span class="font-mono">{{ changeText(c) }}</span>
          <span v-if="c.basis" class="text-muted-foreground"> · {{ basisText(c) }}</span>
        </li>
      </ul>
      <p class="text-2xs text-muted-foreground leading-relaxed">{{ t('routing.proposal.hint') }}</p>
    </div>

    <ul v-if="policies" class="divide-y divide-border/40">
      <li v-for="(v, i) in policies.versions" :key="v.version" class="space-y-1 py-2 first:pt-0 last:pb-0">
        <div class="flex flex-wrap items-center gap-2 text-xs">
          <span class="font-semibold tabular-nums">v{{ v.version }}</span>
          <Badge>{{ sourceLabel(v.source) }}</Badge>
          <Badge v-if="i === 0" tone="primary">{{ t('routing.versions.current') }}</Badge>
          <span class="text-2xs text-muted-foreground">{{ new Date(v.created_at).toLocaleString() }}</span>
          <span class="ml-auto flex gap-1.5">
            <Button variant="ghost" size="xs" @click="viewing = v">{{ t('routing.versions.viewText') }}</Button>
            <Button v-if="i > 0" variant="outline" size="xs" :disabled="busy" @click="goBack(v)">
              {{ t('routing.versions.revert') }}
            </Button>
          </span>
        </div>
        <ul v-if="v.changes.length" class="space-y-0.5 text-2xs">
          <li v-for="c in v.changes" :key="c.kind">
            <span class="font-mono">{{ changeText(c) }}</span>
            <span v-if="c.basis" class="text-muted-foreground"> · {{ basisText(c) }}</span>
          </li>
        </ul>
        <p v-else class="text-2xs text-muted-foreground">{{ t('routing.versions.noChanges') }}</p>
      </li>
    </ul>

    <Modal :open="!!viewing" size="lg" scroll-body :title="viewing ? t('routing.versions.textTitle', { n: viewing.version }) : ''" @close="viewing = null">
      <pre class="whitespace-pre-wrap break-words p-4 font-mono text-2xs leading-relaxed">{{ viewing?.text }}</pre>
    </Modal>
  </Card>
</template>
