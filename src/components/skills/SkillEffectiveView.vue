<script setup lang="ts">
// The Skills screen's "What the agent sees" tab: pick a project and an engine,
// see the skills an agent run there loads, read from the disk — its global
// (HOME) folder and the project's folder, Puppeteer's links and anyone else's
// skills alike. A skill a global one of the same name hides is struck through;
// a library skill with Doctor errors or warnings gets the matrix's badge.
// Read-only: switching happens on the Allocation tab.
import { computed, onActivated, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertCircle, AlertTriangle, Lock, RotateCw, Search } from 'lucide-vue-next'
import { useProjectsStore } from '@/stores/projects'
import { useFeaturesStore } from '@/stores/features'
import { AppSelect, Banner, Button, Input, SegmentedControl } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { engineLabel } from '@/lib/skillRows'
import {
  BADGE_SEVERITIES, CMD_DIAGNOSE_SKILLS, SKILL_DOCTOR_FEATURE, doctorBadge, reportsById,
  type BadgeSeverity, type DoctorBadge, type SkillDoctorReport,
} from '@/lib/skillDoctor'
import {
  CMD_LIST_EFFECTIVE_SKILLS, effectiveCounts, effectiveSections,
  type EffectiveSkill, type EngineSkills, type SkillOrigin,
} from '@/lib/skillEffective'

const emit = defineEmits<{ open: [skillId: string] }>()

const { t } = useI18n()
const projectsStore = useProjectsStore()
const features = useFeaturesStore()

const projectId = ref('')
const engineId = ref('claude')
const search = ref('')
const views = ref<EngineSkills[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
let loadSeq = 0

const projectOptions = computed(() => projectsStore.projects.map((p) => ({ value: p.id, label: p.name })))
const engineOptions = computed(() => views.value.map((v) => ({ value: v.engine.id, label: engineLabel(v.engine.id) })))
const view = computed(() => views.value.find((v) => v.engine.id === engineId.value))
const sections = computed(() => effectiveSections(view.value, search.value))
const counts = computed(() => effectiveCounts(view.value))

// The first project until one is picked; a deleted one falls back to it too.
watch(() => projectsStore.projects, (list) => {
  if (!list.some((p) => p.id === projectId.value)) projectId.value = list[0]?.id ?? ''
}, { immediate: true })

async function load() {
  const seq = ++loadSeq
  void loadDoctor()
  if (!projectId.value) {
    views.value = []
    return
  }
  loading.value = true
  error.value = null
  try {
    const result = await invoke<EngineSkills[]>(CMD_LIST_EFFECTIVE_SKILLS, { projectId: projectId.value })
    if (seq !== loadSeq) return
    views.value = result
    if (!result.some((v) => v.engine.id === engineId.value)) engineId.value = result[0]?.engine.id ?? ''
  } catch (e) {
    if (seq === loadSeq) {
      views.value = []
      error.value = String(e)
    }
  } finally {
    if (seq === loadSeq) loading.value = false
  }
}

watch(projectId, () => void load())
onMounted(() => {
  if (!projectsStore.projects.length) void projectsStore.fetchProjects()
  void load()
})
// Back on the cached Skills screen: the folders may have changed meanwhile.
onActivated(() => {
  if (!loading.value) void load()
})

// ── Doctor badges (library skills only: the Doctor checks the library) ──────

const doctorOn = computed(() => features.isOn(SKILL_DOCTOR_FEATURE))
const doctorReports = ref<Record<string, SkillDoctorReport>>({})
let doctorSeq = 0

async function loadDoctor() {
  const seq = ++doctorSeq
  if (!doctorOn.value) {
    doctorReports.value = {}
    return
  }
  try {
    const reports = await invoke<SkillDoctorReport[]>(CMD_DIAGNOSE_SKILLS)
    if (seq === doctorSeq) doctorReports.value = reportsById(reports)
  } catch {
    // A badge is a hint: the list works without it.
    if (seq === doctorSeq) doctorReports.value = {}
  }
}

watch(doctorOn, () => void loadDoctor())

function badgeOf(skill: EffectiveSkill): DoctorBadge | null {
  return skill.skill_id ? doctorBadge(doctorReports.value[skill.skill_id]) : null
}

const badgeStyle: Record<BadgeSeverity, { icon: typeof AlertCircle; class: string }> = {
  error: { icon: AlertCircle, class: 'bg-destructive/10 text-destructive ring-destructive/30' },
  warn: { icon: AlertTriangle, class: 'bg-warning/15 text-warning ring-warning/30' },
}

function badgeTitle(badge: DoctorBadge): string {
  const summary = BADGE_SEVERITIES.filter((s) => badge.counts[s] > 0)
    .map((s) => t(`skillDoctor.severity.${s}`, badge.counts[s]))
    .join(', ')
  return t('skillDoctor.badgeTitle', { summary })
}

const originClass: Record<SkillOrigin, string> = {
  library: 'bg-primary/12 text-primary ring-primary/30',
  git_managed: 'bg-secondary text-muted-foreground ring-border',
  unmanaged: 'bg-transparent text-muted-foreground ring-border',
}
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex flex-wrap items-center gap-2 border-b border-border/60 px-4 py-2">
      <div class="w-56">
        <AppSelect v-model="projectId" :options="projectOptions" :placeholder="t('skills.effective.project')" />
      </div>
      <SegmentedControl v-if="engineOptions.length" v-model="engineId" :options="engineOptions" :stretch="false" />
      <div class="relative w-48">
        <Search class="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" :stroke-width="1.75" />
        <Input v-model="search" :placeholder="t('skills.detail.searchPlaceholder')" class="pl-7" />
      </div>
      <Button
        variant="outline"
        size="icon"
        :disabled="loading || !projectId"
        :title="t('skills.allocation.refresh')"
        :aria-label="t('skills.allocation.refresh')"
        @click="load"
      >
        <RotateCw class="h-3.5 w-3.5" :stroke-width="1.75" :class="{ 'animate-spin': loading }" />
      </Button>
    </div>

    <div class="min-h-0 flex-1 overflow-auto">
      <div v-if="error" class="p-4">
        <Banner tone="destructive">{{ error }}</Banner>
      </div>
      <p v-else-if="!projectsStore.projects.length" class="p-6 text-center text-xs text-muted-foreground">
        {{ t('skills.allocation.noProjects') }}
      </p>
      <div v-else-if="view" class="flex max-w-3xl flex-col gap-4 p-4 text-xs">
        <p class="text-muted-foreground">
          {{ t('skills.effective.summary', { engine: engineLabel(view.engine.id), loaded: counts.loaded, shadowed: counts.shadowed }) }}
          {{ view.home_wins ? t('skills.effective.homeWins') : t('skills.effective.bothLoad') }}
        </p>
        <section v-for="section in sections" :key="section.scope" class="flex flex-col gap-1">
          <h4 class="flex items-baseline gap-2 font-medium">
            {{ t(`skills.effective.scope.${section.scope}`) }}
            <span class="font-mono text-2xs font-normal text-muted-foreground">{{ section.folder }}</span>
          </h4>
          <p v-if="!section.skills.length" class="text-muted-foreground">
            {{ search.trim() ? t('skills.allocation.noMatch') : t('skills.effective.empty') }}
          </p>
          <ul v-else class="divide-y divide-border/40 rounded-md border border-border/60">
            <li
              v-for="skill in section.skills"
              :key="skill.name"
              class="flex flex-wrap items-center gap-1.5 px-3 py-1.5"
              :class="{ 'opacity-70': skill.shadowed }"
            >
              <button
                v-if="skill.skill_id"
                type="button"
                class="truncate font-mono font-medium hover:text-primary hover:underline"
                :class="{ 'line-through': skill.shadowed }"
                @click="emit('open', skill.skill_id)"
              >
                {{ skill.name }}
              </button>
              <span v-else class="truncate font-mono font-medium" :class="{ 'line-through': skill.shadowed }">
                {{ skill.name }}
              </span>
              <button
                v-if="badgeOf(skill)"
                type="button"
                class="inline-flex items-center gap-0.5 rounded-full px-1 py-px text-2xs font-medium ring-1 ring-inset"
                :class="badgeStyle[badgeOf(skill)!.severity].class"
                :title="badgeTitle(badgeOf(skill)!)"
                @click="emit('open', skill.skill_id!)"
              >
                <component :is="badgeStyle[badgeOf(skill)!.severity].icon" class="h-2.5 w-2.5" :stroke-width="2" />
                {{ badgeOf(skill)!.count }}
              </button>
              <span class="ml-auto flex items-center gap-1">
                <span
                  v-if="skill.shadowed"
                  class="rounded-full bg-violet-500/15 px-1.5 py-px text-2xs font-medium text-violet-700 ring-1 ring-inset ring-violet-500/30 dark:text-violet-300"
                  :title="t('skills.effective.shadowedHint')"
                >
                  {{ t('skills.allocation.state.shadowed') }}
                </span>
                <span
                  class="inline-flex items-center gap-1 rounded-full px-1.5 py-px text-2xs font-medium ring-1 ring-inset"
                  :class="originClass[skill.origin]"
                  :title="t(`skills.effective.originHint.${skill.origin}`)"
                >
                  <Lock v-if="skill.origin === 'git_managed'" class="h-2.5 w-2.5" :stroke-width="2" />
                  {{ t(`skills.effective.origin.${skill.origin}`) }}
                </span>
              </span>
            </li>
          </ul>
        </section>
      </div>
    </div>
  </div>
</template>
