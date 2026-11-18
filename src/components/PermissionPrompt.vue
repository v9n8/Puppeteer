<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ShieldAlert, Check, FileEdit, HelpCircle, ChevronLeft, ChevronRight, ClipboardList, AlertTriangle, Bot } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { canRememberPermission } from '@/lib/permissionMemory'
import { isActivationTarget } from '@/lib/permissionKeys'
import { permissionDiffParts } from '@/lib/permissionImpact'
import { engineLabel, permissionOriginParts, type PermissionAgent } from '@/lib/permissionOrigin'
import { useMinuteClock } from '@/composables/useMinuteClock'
import PermissionRequestBody from '@/components/permission/PermissionRequestBody.vue'

const { t, locale } = useI18n()

export interface PermissionRequest {
  run_id: string
  request_id: string
  tool_name: string
  tool_input: unknown
  session_id?: string | null
  cwd?: string | null
  /** Bridge-rendered prompt sentence (e.g. "Claude wants to edit foo.ts"). */
  title?: string | null
  /** Bridge-rendered subtitle describing the effect. */
  description?: string | null
  /** Short noun phrase for the action (e.g. "Edit file"). */
  display_name?: string | null
  /** A broker prompt (gh / ssh / aws …): answered once, never Always. */
  once_only?: boolean
  /** The run's engine ("claude" / "codex"); absent for a broker prompt. */
  engine?: string | null
  /** Model id from the run's latest `system.init`. */
  model?: string | null
  /** Which agent asked; absent when the engine has no sub-agents (Codex). */
  agent?: PermissionAgent | null
  /** RFC 3339 UTC. */
  requested_at?: string | null
}

const props = withDefaults(defineProps<{
  request: PermissionRequest
  /** Why the agent wants this (see permissionPurpose); hidden when empty. */
  purpose?: string | null
  /** `window`: fills its host (pop-out window / drawer). `inline`: sized by its content. */
  layout?: 'window' | 'inline'
  /** Grab focus on mount so the hotkeys work; an inline host turns this off. */
  autofocus?: boolean
  /** Names of tools the user already chose "allow always" for, this session. */
  allowedTools?: string[]
  /** Markdown → HTML renderer (used to show the plan for ExitPlanMode). */
  renderText?: (md: string) => string
}>(), { purpose: null, layout: 'window', autofocus: true })

const emit = defineEmits<{
  decide: [decision: 'allow' | 'deny', remember: boolean]
  answer: [answers: Record<string, string>]
}>()

function asObject(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {}
}

const input = computed(() => asObject(props.request.tool_input))

const hasDiff = computed(() => permissionDiffParts(props.request).length > 0)

/** AskUserQuestion: render the structured questions as selectable options. */
const isQuestions = computed(() => props.request.tool_name === 'AskUserQuestion')

// ExitPlanMode isn't a risky side-effecting tool — it's Claude presenting a plan
// and asking to leave plan mode. Render it as a plan review (Approve / Keep
// planning) instead of the generic "allow this tool call?" permission prompt.
const isPlan = computed(() => props.request.tool_name === 'ExitPlanMode')
// "Always" needs a stable key to store the choice under (see canRememberPermission).
const canRemember = computed(() => canRememberPermission(props.request))
// Codex's elicitation carries no tool name, so its MCP prompts key on the SERVER
// (`mcp__google`) — and the built-in `google` server is Drive + Gmail + Calendar
// in one. "Always" there is therefore much broader than the single action shown:
// it also opts into codex's own `acceptForSession` cache for the rest of the run.
// Spell that out so approving a calendar read never quietly unlocks mail send or
// permanent file deletion.
const broadRememberWarning = computed(
  () => canRemember.value && props.request.tool_name === 'mcp__google',
)
const planText = computed(() => {
  const p = input.value['plan']
  return typeof p === 'string' ? p : ''
})
const planHtml = computed(() =>
  props.renderText && planText.value ? props.renderText(planText.value) : '',
)

interface QuestionOption {
  label: string
  description: string
  preview?: string
}
interface Question {
  question: string
  header: string
  multiSelect: boolean
  options: QuestionOption[]
}

const questions = computed<Question[]>(() => {
  if (!isQuestions.value) return []
  const raw = input.value['questions']
  if (!Array.isArray(raw)) return []
  return raw.map((q) => {
    const o = asObject(q)
    const opts = Array.isArray(o['options'])
      ? (o['options'] as unknown[])
          .map((x) => {
            const oo = asObject(x)
            return {
              label: typeof oo['label'] === 'string' ? (oo['label'] as string) : '',
              description: typeof oo['description'] === 'string' ? (oo['description'] as string) : '',
              preview: typeof oo['preview'] === 'string' ? (oo['preview'] as string) : undefined,
            }
          })
          .filter((x) => x.label)
      : []
    return {
      question: typeof o['question'] === 'string' ? (o['question'] as string) : '',
      header: typeof o['header'] === 'string' ? (o['header'] as string) : '',
      multiSelect: o['multiSelect'] === true,
      options: opts,
    }
  })
})

// Per-question selected option labels + freeform "Other" text, indexed by question.
const selected = ref<string[][]>([])
const other = ref<string[]>([])
// Questions are shown one at a time (a stepper): a long option list scrolls under
// a single pinned question header instead of many stacked sticky headers piling
// up and covering each other. `step` is the active question; `focusedIndex` is
// the keyboard cursor within that question's items (options + the Other input).
const step = ref(0)
const focusedIndex = ref(0)

watch(
  questions,
  (qs) => {
    selected.value = qs.map(() => [])
    other.value = qs.map(() => '')
    step.value = 0
    focusedIndex.value = 0
  },
  { immediate: true },
)

const currentQuestion = computed<Question | undefined>(() => questions.value[step.value])
const stepCount = computed(() => questions.value.length)
const isLastStep = computed(() => step.value >= stepCount.value - 1)
const otherIndex = computed(() => currentQuestion.value?.options.length ?? 0)

function isSelected(label: string): boolean {
  return (selected.value[step.value] ?? []).includes(label)
}

// --- Keyboard focus model (scoped to the current question) -----------------
// Items = the current question's options (index 0..n-1) followed by its single
// "Other" input (index n). ArrowUp/Down walk them, Space toggles, Enter advances.
function isFocused(oi: number): boolean {
  return focusedIndex.value === oi && oi < otherIndex.value
}
function isOtherFocused(): boolean {
  return focusedIndex.value === otherIndex.value
}
// After moving focus: scroll into view; the "Other" stop grabs DOM focus so the
// user can type, otherwise blur any input and return focus to the panel so Space
// toggles options again.
function applyFocus() {
  nextTick(() => {
    const el = rootEl.value?.querySelector<HTMLElement>(`[data-flat="${focusedIndex.value}"]`)
    el?.scrollIntoView({ block: 'nearest' })
    if (isOtherFocused()) {
      el?.focus()
    } else {
      const active = document.activeElement as HTMLElement | null
      if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {
        active.blur()
        rootEl.value?.focus()
      }
    }
  })
}
function moveFocus(delta: number) {
  const len = otherIndex.value + 1 // options + the Other input
  focusedIndex.value = Math.min(len - 1, Math.max(0, focusedIndex.value + delta))
  applyFocus()
}
function toggleFocused() {
  if (focusedIndex.value < otherIndex.value) {
    toggleOption(currentQuestion.value?.options[focusedIndex.value]?.label ?? '')
  }
}

function toggleOption(label: string) {
  const qi = step.value
  const q = currentQuestion.value
  if (!q) return
  // Keep keyboard focus in sync when the user clicks an option.
  const oi = q.options.findIndex((o) => o.label === label)
  if (oi >= 0) focusedIndex.value = oi
  const cur = selected.value[qi] ?? []
  if (q.multiSelect) {
    selected.value[qi] = cur.includes(label) ? cur.filter((l) => l !== label) : [...cur, label]
  } else {
    selected.value[qi] = cur.includes(label) ? [] : [label]
  }
}

// --- Stepper navigation ----------------------------------------------------
function goToStep(i: number) {
  const next = Math.min(stepCount.value - 1, Math.max(0, i))
  if (next === step.value) return
  step.value = next
  focusedIndex.value = 0
  applyFocus()
}
function goPrev() {
  goToStep(step.value - 1)
}
function goNext() {
  if (isLastStep.value) submitAnswers()
  else if (currentAnswered.value) goToStep(step.value + 1)
}

/** Combined answer for a question: selected labels plus any "Other" text. */
function answerFor(qi: number): string {
  const parts = [...(selected.value[qi] ?? [])]
  const free = (other.value[qi] ?? '').trim()
  if (free) parts.push(free)
  return parts.join(', ')
}

const currentAnswered = computed(() => answerFor(step.value).length > 0)

const canSubmit = computed(
  () => questions.value.length > 0 && questions.value.every((_, i) => answerFor(i).length > 0),
)

function submitAnswers() {
  if (!canSubmit.value) return
  const answers: Record<string, string> = {}
  questions.value.forEach((q, i) => {
    answers[q.question] = answerFor(i)
  })
  emit('answer', answers)
}

const headerTitle = computed(() => {
  if (isPlan.value) return t('permission.headerPlanFinished')
  return props.request.title || (isQuestions.value
    ? t('permission.headerQuestion')
    : engineLabel(props.request.engine, (engine) => t('permission.headerRequest', { engine }), t('permission.headerRequestNoEngine')))
})

// Who asked (engine · model · agent · time); the relative part follows a minute clock.
const now = useMinuteClock()
const originParts = computed(() =>
  permissionOriginParts(props.request, now.value, locale.value, {
    justNow: t('permission.by.justNow'),
    main: t('permission.by.main'),
    sub: t('permission.by.sub'),
    subNamed: (name) => t('permission.by.subNamed', { name }),
  }),
)

// --- Hotkeys ---------------------------------------------------------------
// The panel grabs focus when it appears so its keydown handler receives keys
// without stealing them from the composer when the user clicks back into it.
const rootEl = ref<HTMLElement | null>(null)
onMounted(() => {
  if (props.autofocus) rootEl.value?.focus()
})

function isEditableTarget(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null
  if (!t) return false
  return t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable
}

function onKeydown(e: KeyboardEvent) {
  if (e.isComposing || e.keyCode === 229) return // mid-IME composition
  const editable = isEditableTarget(e)
  // Enter on a focused button / <summary> / link (reached with Tab) activates that
  // element; it must not also count as "Allow", or Tab → "Deny" → Enter would allow.
  if (e.key === 'Enter' && !isQuestions.value && isActivationTarget(e.target)) return

  if (isQuestions.value) {
    switch (e.key) {
      case 'Enter':
        e.preventDefault()
        goNext() // advance to the next question, or submit on the last one
        break
      case 'Escape':
        e.preventDefault()
        emit('decide', 'deny', false)
        break
      // ArrowUp/Down also work from the single-line "Other" input (where they'd
      // otherwise be no-ops), so the keyboard can move into and back out of it.
      case 'ArrowDown':
        e.preventDefault()
        moveFocus(1)
        break
      case 'ArrowUp':
        e.preventDefault()
        moveFocus(-1)
        break
      case 'ArrowRight':
        if (editable) return // let the cursor move within the "Other" field
        e.preventDefault()
        goToStep(step.value + 1)
        break
      case 'ArrowLeft':
        if (editable) return
        e.preventDefault()
        goToStep(step.value - 1)
        break
      case ' ':
      case 'Spacebar':
        if (editable) return // typing a space in "Other"
        e.preventDefault() // also stops the panel from scrolling
        toggleFocused()
        break
    }
    return
  }

  // Permission mode: Enter = allow once, Shift+Enter = allow always, Esc = deny.
  // Plan mode (ExitPlanMode): Enter = approve (never auto-allow — a plan review
  // isn't a tool to remember), Esc = keep planning.
  switch (e.key) {
    case 'Enter':
      e.preventDefault()
      emit('decide', 'allow', canRemember.value && e.shiftKey)
      break
    case 'Escape':
      e.preventDefault()
      emit('decide', 'deny', false)
      break
  }
}
</script>

<template>
  <!-- Fills its host container (a right-side drawer in the run view, or the
       pop-out window), so the user can still read chat history / switch sessions
       while a permission request or question is pending. -->
  <div
    ref="rootEl"
    tabindex="-1"
    class="prompt-attention flex min-h-0 flex-col border-t-2 bg-card outline-none"
    :class="[layout === 'window' ? 'h-full' : '', isQuestions || isPlan ? 'border-t-info/70' : 'border-t-warning/70']"
    role="dialog"
    :aria-label="headerTitle"
    @keydown="onKeydown"
  >
    <!-- Questions get a slim single-line header (each question carries its own
         label/text below); permission keeps the fuller title + description. -->
    <div
      class="flex items-center gap-2.5 px-5 border-b border-border shrink-0"
      :class="isQuestions ? 'py-2' : 'py-3 items-start'"
    >
        <component
          :is="isQuestions ? HelpCircle : isPlan ? ClipboardList : hasDiff ? FileEdit : ShieldAlert"
          class="shrink-0"
          :class="[isQuestions ? 'h-4 w-4' : 'h-5 w-5 mt-0.5', isQuestions || isPlan ? 'text-info' : 'text-warning']"
          :stroke-width="1.75"
        />
        <div class="flex-1 min-w-0">
          <h2
            class="text-[calc(0.9375rem*var(--text-scale))] font-semibold text-foreground"
            :class="isQuestions ? 'leading-tight' : 'leading-[1.4]'"
          >{{ headerTitle }}</h2>
          <p
            v-if="!isQuestions && !isPlan && originParts.length"
            class="mt-0.5 flex min-w-0 items-start gap-1.5 text-xs text-muted-foreground"
          >
            <Bot class="h-4 w-4 shrink-0" :stroke-width="1.75" aria-hidden="true" />
            <span class="min-w-0 break-words">
              <template v-for="(part, i) in originParts" :key="part.key">
                <template v-if="i > 0"> · </template>
                <span :title="part.title">{{ part.text }}</span>
              </template>
            </span>
          </p>
          <p v-if="isPlan" class="text-xs text-foreground/60 mt-0.5">
            {{ t('permission.planReviewHint') }}
          </p>
          <p v-else-if="!isQuestions && request.description" class="text-xs text-foreground/60 mt-0.5">
            {{ request.description }}
          </p>
          <p v-else-if="!isQuestions" class="text-xs text-foreground/60 mt-0.5">
            {{ t('permission.allowHint') }}
          </p>
        </div>
      </div>

      <!-- AskUserQuestion: one question at a time (stepper). The question header
           is pinned; only its option list scrolls — no stacked sticky headers. -->
      <div v-if="isQuestions && currentQuestion" class="flex-1 min-h-0 flex flex-col">
        <div class="px-5 pt-3 pb-2.5 border-b border-border shrink-0 space-y-1">
          <div class="flex items-center gap-2 flex-wrap">
            <span
              v-if="currentQuestion.header"
              class="text-2xs uppercase tracking-wider rounded bg-info/15 text-info px-1.5 py-0.5"
            >{{ currentQuestion.header }}</span>
            <span class="text-2xs uppercase tracking-wider text-foreground/40">
              {{ currentQuestion.multiSelect ? t('permission.selectAll') : t('permission.selectOne') }}
            </span>
          </div>
          <p class="text-sm font-medium text-foreground">{{ currentQuestion.question }}</p>
        </div>

        <div class="flex-1 min-h-0 overflow-auto px-5 py-3 space-y-1.5">
          <button
            v-for="(opt, oi) in currentQuestion.options"
            :key="oi"
            type="button"
            :data-flat="oi"
            class="w-full text-left rounded-md border px-3 py-2 transition-colors"
            :class="[
              isSelected(opt.label)
                ? 'border-primary bg-primary/10'
                : 'border-border bg-foreground/5 hover:bg-foreground/10',
              isFocused(oi) ? 'ring-2 ring-primary/60 ring-offset-1 ring-offset-card' : '',
            ]"
            @click="toggleOption(opt.label)"
          >
            <div class="flex items-start gap-2.5">
              <span
                class="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center border"
                :class="[
                  currentQuestion.multiSelect ? 'rounded' : 'rounded-full',
                  isSelected(opt.label)
                    ? 'border-primary bg-primary text-primary-foreground'
                    : 'border-foreground/30',
                ]"
              >
                <Check v-if="isSelected(opt.label)" class="h-3 w-3" :stroke-width="3" />
              </span>
              <span class="min-w-0 flex-1">
                <span class="block text-sm text-foreground">{{ opt.label }}</span>
                <span v-if="opt.description" class="block text-xs text-foreground/60 mt-0.5">
                  {{ opt.description }}
                </span>
                <pre
                  v-if="opt.preview"
                  class="mt-1.5 rounded bg-foreground/5 border border-border px-2 py-1.5 text-2xs font-mono text-foreground/70 whitespace-pre-wrap break-words max-h-48 overflow-auto"
                >{{ opt.preview }}</pre>
              </span>
            </div>
          </button>
          <input
            v-model="other[step]"
            type="text"
            :data-flat="otherIndex"
            :placeholder="t('permission.otherPlaceholder')"
            class="w-full rounded-md border border-border bg-foreground/5 px-3 py-2 text-sm text-foreground placeholder:text-foreground/40 focus:border-primary focus:outline-none"
            :class="isOtherFocused() ? 'ring-2 ring-primary/60 ring-offset-1 ring-offset-card' : ''"
            @focus="focusedIndex = otherIndex"
          />
        </div>
      </div>

      <!-- ExitPlanMode: show the proposed plan (rendered markdown), not a raw
           tool-input dump. -->
      <div v-else-if="isPlan" class="flex-1 min-h-0 px-5 py-4 overflow-auto">
        <div
          v-if="planHtml"
          v-html="planHtml"
          class="markdown-output text-sm leading-relaxed text-foreground"
        />
        <pre
          v-else
          class="text-xs font-mono text-foreground/80 whitespace-pre-wrap break-words"
        >{{ planText || t('permission.noPlanDetails') }}</pre>
      </div>

      <div v-else class="flex-1 min-h-0 px-5 py-4 overflow-auto">
        <PermissionRequestBody :request="request" :purpose="purpose" />
      </div>

      <div class="@container/footer px-5 py-3 border-t border-border shrink-0">
        <div
          v-if="broadRememberWarning"
          class="flex items-start gap-2 mb-2 text-2xs leading-snug text-warning"
        >
          <AlertTriangle class="h-3 w-3 mt-0.5 shrink-0" :stroke-width="1.75" />
          <span>{{ t('permission.broadScopeWarning') }}</span>
        </div>
        <div class="flex flex-col gap-2">
          <span class="hidden text-2xs text-foreground/40 @[26rem]/footer:block">
            <template v-if="isQuestions">{{ t('permission.hintQuestions') }}</template>
            <template v-else-if="isPlan">{{ t('permission.hintPlan') }}</template>
            <template v-else>{{ t('permission.hintPermission') }}</template>
          </span>
          <!-- Action buttons: stack full-width in a narrow panel; wrap to a second row when four do not fit. -->
          <div class="flex flex-col gap-2 @[26rem]/footer:flex-row @[26rem]/footer:flex-wrap @[26rem]/footer:items-center @[26rem]/footer:justify-end">
            <template v-if="isQuestions">
              <span
                v-if="stepCount > 1"
                class="self-center text-2xs font-medium text-foreground/50 tabular-nums"
              >{{ step + 1 }}/{{ stepCount }}</span>
              <Button
                v-if="stepCount > 1"
                variant="outline"
                :disabled="step === 0"
                class="w-full @[26rem]/footer:w-auto"
                @click="goPrev"
              >
                <ChevronLeft class="h-3.5 w-3.5" :stroke-width="2" /> {{ t('common.back') }}
              </Button>
              <Button variant="outline" class="w-full @[26rem]/footer:w-auto" @click="emit('decide', 'deny', false)">
                {{ t('permission.controller.cancel') }}
              </Button>
              <Button
                v-if="!isLastStep"
                variant="primary"
                :disabled="!currentAnswered"
                class="w-full @[26rem]/footer:w-auto"
                @click="goNext"
              >
                {{ t('permission.next') }} <ChevronRight class="h-3.5 w-3.5" :stroke-width="2" />
              </Button>
              <Button v-else variant="primary" :disabled="!canSubmit" class="w-full @[26rem]/footer:w-auto" @click="submitAnswers">
                <Check class="h-3.5 w-3.5" :stroke-width="2" /> {{ t('permission.submit') }}
              </Button>
            </template>
            <template v-else-if="isPlan">
              <Button
                variant="outline"
                class="w-full @[26rem]/footer:w-auto"
                @click="emit('decide', 'deny', false)"
              >
                {{ t('permission.keepPlanning') }}
              </Button>
              <Button
                variant="primary"
                class="w-full @[26rem]/footer:w-auto"
                @click="emit('decide', 'allow', false)"
              >
                <Check class="h-3.5 w-3.5" :stroke-width="2" /> {{ t('permission.approve') }}
              </Button>
            </template>
            <template v-else>
              <Button
                variant="outline"
                class="w-full @[26rem]/footer:w-auto"
                @click="emit('decide', 'deny', false)"
              >
                {{ t('permission.deny') }}
              </Button>
              <Button
                v-if="canRemember"
                variant="outline"
                class="w-full @[26rem]/footer:w-auto"
                @click="emit('decide', 'deny', true)"
                :title="t('permission.denyAlwaysTitle')"
              >
                {{ t('permission.denyAlways') }}
              </Button>
              <Button
                v-if="canRemember"
                variant="primary"
                class="w-full @[26rem]/footer:w-auto"
                @click="emit('decide', 'allow', true)"
                :title="t('permission.allowAlwaysTitle')"
              >
                {{ t('permission.always') }}
              </Button>
              <Button
                variant="primary"
                class="w-full @[26rem]/footer:w-auto"
                @click="emit('decide', 'allow', false)"
              >
                {{ t('permission.allow') }}
              </Button>
            </template>
          </div>
        </div>
      </div>
    </div>
</template>

<style scoped>
/* Slide up + brief glow when the panel appears (incl. each new request, since
   RunView keys it by request_id) so it catches the eye without a modal. */
.prompt-attention {
  animation: prompt-in 0.28s ease-out;
}
@keyframes prompt-in {
  0% {
    transform: translateX(16px);
    opacity: 0;
  }
  40% {
    box-shadow: inset 2px 0 0 0 hsl(var(--info) / 0.45);
  }
  100% {
    transform: translateX(0);
    opacity: 1;
  }
}
</style>
