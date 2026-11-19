<script setup lang="ts">
// Settings → "Routing" (QO-06): the three routing switches (`routing.*` keys of
// the settings form) and the routing report of the last 7 / 30 days
// (`get_routing_report`, src-tauri/src/runs/routing/report.rs).
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Route, ChartBar } from 'lucide-vue-next'
import { Card, InfoTip, SegmentedControl, ToggleSwitch, type SegmentedOption } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import {
  ROUTING_PERIODS, adoptionRates, routingReportCells, switchOn,
  type RoutingPeriod, type RoutingReport, type RoutingSwitch,
} from '@/lib/routing'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'

const { t, te } = useI18n()
const { settings } = useSettingsForm()

const SWITCHES: { key: RoutingSwitch; id: string; label: string; hint: string }[] = [
  { key: 'routing.policy_enabled', id: 'routing-policy', label: 'routing.switches.policy', hint: 'routing.switches.policyHint' },
  { key: 'routing.escalation_enabled', id: 'routing-escalation', label: 'routing.switches.escalation', hint: 'routing.switches.escalationHint' },
  { key: 'routing.auto_update', id: 'routing-autoUpdate', label: 'routing.switches.autoUpdate', hint: 'routing.switches.autoUpdateHint' },
]

function flip(key: RoutingSwitch) {
  settings.value[key] = switchOn(settings.value[key]) ? 'false' : 'true'
}

const period = ref<RoutingPeriod>(7)
const periodOptions = computed<SegmentedOption<RoutingPeriod>[]>(() =>
  ROUTING_PERIODS.map((days) => ({ value: days, label: t('routing.report.days', { n: days }) })),
)
const report = ref<RoutingReport | null>(null)
const loadError = ref('')

async function loadReport() {
  try {
    report.value = await invoke<RoutingReport>('get_routing_report', { days: period.value })
    loadError.value = ''
  } catch (e) {
    loadError.value = errorText(e, t)
  }
}
onMounted(loadReport)
watch(period, loadReport)

const rows = computed(() => (report.value?.groups ?? []).map((g) => ({ key: `${g.task_kind}|${g.model}|${g.effort}`, ...routingReportCells(g) })))
const adoption = computed(() => (report.value ? adoptionRates(report.value.adoption) : null))
// Effort levels read as the effort pickers do; anything else ("—", an id) as stored.
const effortLabel = (e: string) => (te(`settings.ai.effort.${e}`) ? t(`settings.ai.effort.${e}`) : e)
const COLUMNS = ['kind', 'model', 'effort', 'runs', 'success', 'avgTokens', 'retried', 'byAgent'] as const
</script>

<template>
  <div class="max-w-4xl space-y-4">
    <Card body-class="p-4 space-y-4">
      <template #header>
        <Route class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('routing.title') }}</span>
        <InfoTip :text="t('routing.hint')" />
      </template>
      <SettingsRow v-for="s in SWITCHES" :key="s.key" :search-id="s.id" :label="t(s.label)" :hint="t(s.hint)">
        <div class="flex min-[901px]:justify-end">
          <ToggleSwitch size="md" :checked="switchOn(settings[s.key])" :label="t(s.label)" @toggle="flip(s.key)" />
        </div>
      </SettingsRow>
    </Card>

    <Card body-class="p-4 space-y-3">
      <template #header>
        <ChartBar class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('routing.report.title') }}</span>
        <InfoTip :text="t('routing.report.hint')" />
      </template>
      <div data-settings-id="routing-report" class="w-48">
        <SegmentedControl v-model="period" :options="periodOptions" radio :aria-label="t('routing.report.title')" />
      </div>
      <p v-if="loadError" class="text-2xs text-destructive">{{ loadError }}</p>
      <p v-else-if="report && !rows.length" class="text-2xs text-muted-foreground">{{ t('routing.report.empty') }}</p>
      <div v-else-if="rows.length" class="overflow-x-auto">
        <table class="w-full text-xs">
          <thead class="text-2xs uppercase tracking-wider text-muted-foreground">
            <tr>
              <th
                v-for="(c, i) in COLUMNS"
                :key="c"
                class="pb-1 font-medium"
                :class="i < 3 ? 'text-left' : 'text-right'"
              >{{ t(`routing.report.columns.${c}`) }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="r in rows" :key="r.key" class="border-t border-border/40">
              <td class="py-1 font-mono">{{ r.kind }}</td>
              <td class="py-1 font-mono">{{ r.model }}</td>
              <td class="py-1">{{ effortLabel(r.effort) }}</td>
              <td class="py-1 text-right tabular-nums">{{ r.runs }}</td>
              <td class="py-1 text-right tabular-nums">{{ r.success }}</td>
              <td class="py-1 text-right tabular-nums">{{ r.avgTokens }}</td>
              <td class="py-1 text-right tabular-nums">{{ r.retried }}</td>
              <td class="py-1 text-right tabular-nums">{{ r.byAgent }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <p v-if="report && adoption" class="text-2xs text-muted-foreground leading-relaxed">
        {{ t('routing.report.adoption', { calls: report.adoption.calls, model: adoption.model, effort: adoption.effort, both: adoption.both }) }}
      </p>
    </Card>
  </div>
</template>
