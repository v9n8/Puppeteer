<script setup lang="ts">
// Settings → "Features": one switch per module; clicking a module lists its
// props. Locked props are dimmed with the reason, a prop missing a dependency
// says what it lacks, and "Restore defaults" first lists what it would turn off
// (a `reset_features` dry run). A switch listed in `CONFIRM_ON` asks before it
// goes on. The switches are local-only, so a remote-desktop window shows them
// read-only.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, Loader2, RotateCcw, ToggleRight } from 'lucide-vue-next'
import { Banner, Button, Card, InfoTip, ToggleSwitch } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { invoke } from '@/lib/tauri'
import { isRemoteMode } from '@/lib/ipc/remote'
import { CMD_RESET_FEATURES, CMD_SET_FEATURE, confirmOnKey, featureRows, type FeatureTree } from '@/lib/featureGate'
import { useFeaturesStore } from '@/stores/features'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const features = useFeaturesStore()

const readOnly = isRemoteMode()
const rows = computed(() => featureRows(features.tree, readOnly))
const expanded = ref(new Set<string>())
/** Id being written, or `reset`. */
const busy = ref<string | null>(null)

function toggleExpanded(module: string) {
  const next = new Set(expanded.value)
  if (!next.delete(module)) next.add(module)
  expanded.value = next
}

/** Display name of a module or prop id. */
function nameOf(id: string): string {
  return features.tree.some((m) => m.module === id) ? t(`features.module.${id}`) : t(`features.prop.${id}`)
}

function showError(e: unknown) {
  const msg = String(e)
  toast.error(msg.includes('remote_denied_local_only') ? t('features.settings.remoteReadOnly') : msg)
}

async function setFeature(id: string, enabled: boolean) {
  if (busy.value) return
  busy.value = id
  try {
    const ask = confirmOnKey(id, enabled)
    if (ask && !(await confirm({ title: t(`${ask}.title`), message: t(`${ask}.message`), confirmLabel: t(`${ask}.confirm`) }))) {
      return
    }
    features.tree = await invoke<FeatureTree>(CMD_SET_FEATURE, { id, enabled })
  } catch (e) {
    showError(e)
  } finally {
    busy.value = null
  }
}

async function resetDefaults() {
  if (busy.value) return
  busy.value = 'reset'
  try {
    const off = await invoke<string[]>(CMD_RESET_FEATURES, { dryRun: true })
    const ok = await confirm({
      title: t('features.settings.resetTitle'),
      message: off.length
        ? t('features.settings.resetMessage', { list: off.map((id) => `• ${nameOf(id)}`).join('\n') })
        : t('features.settings.resetNothingOff'),
      confirmLabel: t('features.settings.reset'),
    })
    if (!ok) return
    await invoke<string[]>(CMD_RESET_FEATURES, { dryRun: false })
    await features.refresh()
    toast.success(t('features.settings.resetDone'))
  } catch (e) {
    showError(e)
  } finally {
    busy.value = null
  }
}
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <ToggleRight class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('features.settings.title') }}</span>
      <InfoTip :text="t('features.settings.help')" />
      <Button
        v-if="!readOnly"
        variant="outline"
        size="xs"
        class="ml-auto"
        :disabled="busy !== null || !rows.length"
        @click="resetDefaults"
      >
        <Loader2 v-if="busy === 'reset'" class="h-3 w-3 animate-spin" :stroke-width="2" />
        <RotateCcw v-else class="h-3 w-3" :stroke-width="1.75" />
        {{ t('features.settings.reset') }}
      </Button>
    </template>

    <Banner v-if="readOnly" tone="info">{{ t('features.settings.remoteReadOnly') }}</Banner>

    <div v-if="!features.loaded" class="h-24 animate-pulse rounded-md border border-border bg-card" />

    <p v-else-if="!rows.length" class="text-2xs text-destructive">
      {{ t('features.settings.loadError', { error: features.error ?? '' }) }}
    </p>

    <div v-else class="divide-y divide-border/60 rounded-lg border border-border/60">
      <div v-for="m in rows" :key="m.module">
        <div class="flex items-center gap-3 px-3.5 py-2.5">
          <button
            type="button"
            class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 text-left"
            :aria-expanded="expanded.has(m.module)"
            @click="toggleExpanded(m.module)"
          >
            <ChevronRight
              class="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform"
              :class="expanded.has(m.module) && 'rotate-90'"
              :stroke-width="1.75"
            />
            <span class="truncate text-sm font-medium text-foreground">{{ t(`features.module.${m.module}`) }}</span>
            <span
              v-if="m.locked"
              class="shrink-0 rounded bg-muted px-1.5 py-px text-2xs text-muted-foreground"
            >{{ t('features.settings.core') }}</span>
            <span class="ml-auto shrink-0 text-2xs tabular-nums text-muted-foreground">
              {{ t('features.settings.onCount', { on: m.onCount, total: m.props.length }) }}
            </span>
          </button>
          <ToggleSwitch
            size="md"
            :checked="m.checked"
            :disabled="!m.editable || (busy !== null && busy !== m.module)"
            :loading="busy === m.module"
            :label="t(`features.module.${m.module}`)"
            @toggle="setFeature(m.module, !m.checked)"
          />
        </div>

        <ul v-if="expanded.has(m.module)" class="space-y-2 border-t border-border/40 bg-muted/20 py-2 pl-9 pr-3.5">
          <li v-for="p in m.props" :key="p.id" class="flex items-start gap-3">
            <div class="min-w-0 flex-1" :class="p.muted && 'opacity-60'">
              <p class="text-xs text-foreground">{{ t(`features.prop.${p.id}`) }}</p>
              <p
                v-if="p.reasonKey"
                class="mt-0.5 text-2xs"
                :class="p.reasonKey === 'features.reason.missing_dependency' ? 'text-warning' : 'text-muted-foreground'"
              >
                {{ t(p.reasonKey, { deps: p.degraded.map(nameOf).join(', ') }) }}
              </p>
            </div>
            <ToggleSwitch
              :checked="p.checked"
              :disabled="!p.editable || (busy !== null && busy !== p.id)"
              :loading="busy === p.id"
              :label="t(`features.prop.${p.id}`)"
              @toggle="setFeature(p.id, !p.checked)"
            />
          </li>
        </ul>
      </div>
    </div>
  </Card>
</template>
