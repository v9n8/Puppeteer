<script setup lang="ts">
// Skill eval of one library skill (`run_skill_eval`): the user writes sample
// requests, each expected to load this skill or not, sets the spend caps, and
// runs them against the translate engine's model. Cases and the last result
// live as long as the skill stays open; nothing is stored.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, Trash2 } from 'lucide-vue-next'
import { AppSelect, Badge, Banner, Button, Input, Modal, Textarea } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import {
  CMD_RUN_SKILL_EVAL, DEFAULT_EVAL_BUDGET, MAX_CASE_PROMPT_CHARS, MAX_EVAL_CASES, caseProblem, evalArgs, evalSummary,
  newCaseId,
  type EvalBudgetForm, type EvalCase, type EvalCaseResult, type SkillEvalRun,
} from '@/lib/skillEval'

const props = defineProps<{
  open: boolean
  skillId: string
  skillName: string
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  finished: [run: SkillEvalRun]
}>()

const { t } = useI18n()

const cases = ref<EvalCase[]>([
  { id: 'c1', prompt: '', expect: 'activate' },
  { id: 'c2', prompt: '', expect: 'skip' },
])
const budget = ref<EvalBudgetForm>({ ...DEFAULT_EVAL_BUDGET })
const running = ref(false)
const error = ref<string | null>(null)
const run = ref<SkillEvalRun | null>(null)

const expectOptions = computed(() => [
  { value: 'activate', label: t('skillEval.expect.activate') },
  { value: 'skip', label: t('skillEval.expect.skip') },
])

const problem = computed(() => caseProblem(cases.value))
const problemText = computed(() => (problem.value ? t(`skillEval.problem.${problem.value.key}`, problem.value) : ''))
const summary = computed(() => (run.value ? evalSummary(run.value) : null))

/** A scored case passes or fails; the others were skipped (a cap) or failed to run. */
function tone(r: EvalCaseResult): 'success' | 'error' | 'warning' | 'neutral' {
  if (r.status === 'scored') return r.pass ? 'success' : 'error'
  return r.status === 'error' ? 'warning' : 'neutral'
}

function addCase() {
  if (cases.value.length >= MAX_EVAL_CASES) return
  cases.value.push({ id: newCaseId(cases.value), prompt: '', expect: 'activate' })
}

function removeCase(id: string) {
  cases.value = cases.value.filter((c) => c.id !== id)
}

function setNumber(key: keyof EvalBudgetForm, value: string) {
  budget.value = { ...budget.value, [key]: value.trim() === '' ? NaN : Number(value) }
}

async function runEval() {
  if (running.value || problem.value) return
  running.value = true
  error.value = null
  try {
    const result = await invoke<SkillEvalRun>(CMD_RUN_SKILL_EVAL, evalArgs(props.skillId, cases.value, budget.value))
    run.value = result
    emit('finished', result)
  } catch (e) {
    error.value = String(e)
  } finally {
    running.value = false
  }
}
</script>

<template>
  <Modal
    :open="open"
    size="xl"
    scroll-body
    :dismiss-on-overlay="false"
    :title="t('skillEval.title', { name: skillName })"
    @update:open="(v: boolean) => emit('update:open', v)"
  >
    <div class="space-y-4 p-4 text-xs">
      <p class="text-muted-foreground">{{ t('skillEval.intro') }}</p>

      <section class="space-y-2">
        <div class="flex items-center gap-2">
          <h4 class="flex-1 text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
            {{ t('skillEval.cases', { count: cases.length, max: MAX_EVAL_CASES }) }}
          </h4>
          <Button variant="outline" size="xs" :disabled="cases.length >= MAX_EVAL_CASES" @click="addCase">
            <Plus class="h-3 w-3" :stroke-width="1.75" />
            {{ t('skillEval.addCase') }}
          </Button>
        </div>
        <div v-for="(c, i) in cases" :key="c.id" class="flex items-start gap-2">
          <span class="w-5 shrink-0 pt-1.5 text-right font-mono text-2xs text-muted-foreground">{{ i + 1 }}</span>
          <Textarea
            v-model="c.prompt"
            rows="2"
            class="min-w-0 flex-1"
            :maxlength="MAX_CASE_PROMPT_CHARS"
            :placeholder="t('skillEval.promptPlaceholder')"
          />
          <AppSelect
            :model-value="c.expect"
            :options="expectOptions"
            class="w-36 shrink-0"
            @update:model-value="(v: string) => (c.expect = v === 'skip' ? 'skip' : 'activate')"
          />
          <Button
            variant="destructive-ghost"
            size="icon-sm"
            :title="t('skillEval.removeCase')"
            :aria-label="t('skillEval.removeCase')"
            @click="removeCase(c.id)"
          >
            <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </div>
      </section>

      <section class="space-y-2">
        <h4 class="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('skillEval.budget') }}</h4>
        <div class="grid grid-cols-3 gap-3">
          <label class="block space-y-1">
            <span class="text-2xs text-muted-foreground">{{ t('skillEval.maxCalls') }}</span>
            <Input
              type="number" min="1" :max="MAX_EVAL_CASES"
              :model-value="Number.isNaN(budget.maxCalls) ? '' : budget.maxCalls"
              @update:model-value="(v: string) => setNumber('maxCalls', v)"
            />
          </label>
          <label class="block space-y-1">
            <span class="text-2xs text-muted-foreground">{{ t('skillEval.maxCostUsd') }}</span>
            <Input
              type="number" min="0.01" step="0.05"
              :model-value="Number.isNaN(budget.maxCostUsd) ? '' : budget.maxCostUsd"
              @update:model-value="(v: string) => setNumber('maxCostUsd', v)"
            />
          </label>
          <label class="block space-y-1">
            <span class="text-2xs text-muted-foreground">{{ t('skillEval.maxMinutes') }}</span>
            <Input
              type="number" min="1" step="1"
              :model-value="Number.isNaN(budget.maxMinutes) ? '' : budget.maxMinutes"
              @update:model-value="(v: string) => setNumber('maxMinutes', v)"
            />
          </label>
        </div>
        <p class="text-2xs text-muted-foreground">{{ t('skillEval.budgetHint') }}</p>
      </section>

      <p v-if="error" class="text-destructive">{{ t('skillEval.failed', { error }) }}</p>

      <section v-if="run && summary" class="space-y-2">
        <h4 class="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">{{ t('skillEval.results') }}</h4>
        <p>
          {{ t('skillEval.summary', { passed: summary.passed, scored: summary.scored, skipped: summary.skipped, errors: summary.errors }) }}
          ·
          {{ t(run.cost_estimated ? 'skillEval.costEstimated' : 'skillEval.cost', { cost: run.cost_usd.toFixed(4), calls: run.calls }) }}
          ·
          <span class="font-mono">{{ run.engine }}{{ run.model ? ` / ${run.model}` : '' }}</span>
        </p>
        <Banner v-if="run.stopped_by" tone="warning" class="text-xs">
          {{ t(`skillEval.stopped.${run.stopped_by}`) }}
        </Banner>
        <ul class="space-y-1.5">
          <li v-for="r in run.results" :key="r.case_id" class="flex items-start gap-2 rounded border border-border/60 px-2 py-1.5">
            <Badge :tone="tone(r)" size="xs" class="mt-0.5">
              {{ r.status === 'scored' ? t(r.pass ? 'skillEval.pass' : 'skillEval.fail') : t(`skillEval.status.${r.status}`) }}
            </Badge>
            <div class="min-w-0 flex-1 space-y-0.5">
              <p class="break-words text-foreground/90">{{ r.prompt }}</p>
              <p class="text-2xs text-muted-foreground">
                {{ t(`skillEval.expect.${r.expect}`) }}
                <template v-if="r.status === 'scored'">
                  · {{ t('skillEval.chosen', { skill: r.chosen ?? t('skillEval.none') }) }}
                  <template v-if="r.reason"> · {{ r.reason }}</template>
                </template>
                <template v-else-if="r.error"> · {{ r.error }}</template>
              </p>
            </div>
          </li>
        </ul>
      </section>
    </div>

    <template #footer>
      <p v-if="problemText" class="mr-auto text-2xs text-muted-foreground">{{ problemText }}</p>
      <Button variant="ghost" size="sm" @click="emit('update:open', false)">{{ t('common.close') }}</Button>
      <Button size="sm" :disabled="running || !!problem" @click="runEval">
        {{ running ? t('skillEval.running') : t('skillEval.run') }}
      </Button>
    </template>
  </Modal>
</template>
