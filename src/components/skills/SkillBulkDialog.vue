<script setup lang="ts">
// Bulk switch from the Allocation tab: pick skills (or a skill group), projects
// and engines, switch them on or off. "Preview" lists every file that would be
// created or removed and what is skipped; only then can it be applied. Any
// change to the form drops the preview, so what is applied is what was shown.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { FileMinus, FilePlus, Loader2 } from 'lucide-vue-next'
import { useSkillsStore, type Skill } from '@/stores/skills'
import { useSkillGroupsStore } from '@/stores/groups'
import { useFeaturesStore } from '@/stores/features'
import { useToast } from '@/composables/useToast'
import AppSelect from '@/components/AppSelect.vue'
import { Banner, Button, Input, Modal, SegmentedControl } from '@/components/ui'
import { engineLabel, type SkillAllocation } from '@/lib/skillRows'
import {
  bulkPlanByProject,
  bulkRequestReady,
  summarizeBulkPlan,
  toggleId,
  type SkillBulkPlan,
  type SkillBulkRequest,
} from '@/lib/skillBulk'

const props = defineProps<{
  open: boolean
  skills: Skill[]
  allocation: SkillAllocation
}>()
const emit = defineEmits<{ close: []; applied: [] }>()

const { t } = useI18n()
const { toast } = useToast()
const store = useSkillsStore()
const groupsStore = useSkillGroupsStore()
const features = useFeaturesStore()

const byGroup = computed(() => features.isOn('library.groups') && groupsStore.groups.length > 0)
const pickMode = ref<'skills' | 'group'>('skills')
const pickOptions = computed(() => [
  { value: 'skills' as const, label: t('skills.bulk.pickSkills') },
  { value: 'group' as const, label: t('skills.bulk.pickGroup') },
])
const skillSearch = ref('')
const skillIds = ref<string[]>([])
const groupId = ref('')
const projectIds = ref<string[]>([])
const engines = ref<string[]>([])
const action = ref<'on' | 'off'>('on')
const actionOptions = computed(() => [
  { value: 'on' as const, label: t('skills.bulk.switchOn') },
  { value: 'off' as const, label: t('skills.bulk.switchOff') },
])

const allSkillIds = computed(() => props.skills.map((s) => s.id))
const allProjectIds = computed(() => props.allocation.projects.map((p) => p.id))
const allEngines = computed(() => props.allocation.engines.map((e) => e.id))
const shownSkills = computed(() => {
  const q = skillSearch.value.trim().toLowerCase()
  return q ? props.skills.filter((s) => s.name.toLowerCase().includes(q)) : props.skills
})
const groupOptions = computed(() =>
  groupsStore.groups.map((g) => ({ value: g.id, label: `${g.name} (${g.member_count})` })),
)

const request = computed<SkillBulkRequest>(() => {
  const group = byGroup.value && pickMode.value === 'group'
  return {
    skill_ids: group ? [] : skillIds.value,
    group_id: group && groupId.value ? groupId.value : null,
    project_ids: projectIds.value,
    engines: engines.value,
    on: action.value === 'on',
  }
})
const ready = computed(() => bulkRequestReady(request.value))

const plan = ref<SkillBulkPlan | null>(null)
const planning = ref(false)
const applying = ref(false)
const error = ref<string | null>(null)
const summary = computed(() => (plan.value ? summarizeBulkPlan(plan.value) : null))
const sections = computed(() => (plan.value ? bulkPlanByProject(plan.value) : []))

// Opened afresh: every project and engine, nothing picked yet.
watch(
  () => props.open,
  (open) => {
    if (!open) return
    pickMode.value = 'skills'
    skillSearch.value = ''
    skillIds.value = []
    groupId.value = ''
    projectIds.value = [...allProjectIds.value]
    engines.value = [...allEngines.value]
    action.value = 'on'
    plan.value = null
    error.value = null
  },
  { immediate: true },
)
watch(request, () => {
  plan.value = null
  error.value = null
}, { deep: true })

async function preview() {
  if (!ready.value || planning.value) return
  planning.value = true
  error.value = null
  try {
    plan.value = await store.planSkillBulk(request.value)
  } catch (e) {
    error.value = String(e)
  } finally {
    planning.value = false
  }
}

async function apply() {
  if (!plan.value?.changes.length || applying.value) return
  applying.value = true
  try {
    const result = await store.applySkillBulk(request.value)
    if (result.failures.length === 0) {
      toast.success(t('skills.bulk.toastApplied', result.applied))
    } else {
      const details = result.failures.map((f) => `• ${f.project_name}: ${f.error}`).join('\n')
      toast.error(t('skills.bulk.toastPartial', { applied: result.applied, failed: result.failures.length, details }))
    }
    emit('applied')
    emit('close')
  } catch (e) {
    error.value = String(e)
  } finally {
    applying.value = false
  }
}

/** Everything picked, or nothing when everything already is. */
function allOrNone(list: string[], all: string[]): string[] {
  return list.length === all.length ? [] : [...all]
}
</script>

<template>
  <Modal :open="open" size="xl" scroll-body :title="t('skills.bulk.title')" @close="emit('close')">
    <div class="grid gap-4 p-4 text-xs md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <section class="flex min-w-0 flex-col gap-2">
        <div class="flex items-center justify-between gap-2">
          <h4 class="font-medium">{{ t('skills.bulk.skills') }}</h4>
          <SegmentedControl v-if="byGroup" v-model="pickMode" :options="pickOptions" :stretch="false" />
        </div>
        <AppSelect
          v-if="byGroup && pickMode === 'group'"
          v-model="groupId"
          :options="groupOptions"
          :placeholder="t('skills.bulk.groupPlaceholder')"
        />
        <template v-else>
          <div class="flex items-center gap-2">
            <Input v-model="skillSearch" :placeholder="t('skills.detail.searchPlaceholder')" class="flex-1" />
            <Button variant="ghost" size="xs" @click="skillIds = allOrNone(skillIds, allSkillIds)">
              {{ skillIds.length === allSkillIds.length ? t('skills.bulk.none') : t('skills.bulk.all') }}
            </Button>
          </div>
          <div class="max-h-48 overflow-auto rounded-md border border-border/60 p-1">
            <label
              v-for="s in shownSkills"
              :key="s.id"
              class="flex cursor-pointer items-center gap-2 rounded px-2 py-1 hover:bg-muted/50"
            >
              <input
                type="checkbox"
                class="accent-primary"
                :checked="skillIds.includes(s.id)"
                @change="skillIds = toggleId(skillIds, s.id, allSkillIds)"
              >
              <span class="truncate font-mono">{{ s.name }}</span>
            </label>
          </div>
        </template>
      </section>

      <section class="flex min-w-0 flex-col gap-2">
        <div class="flex items-center justify-between gap-2">
          <h4 class="font-medium">{{ t('skills.bulk.projects') }}</h4>
          <Button variant="ghost" size="xs" @click="projectIds = allOrNone(projectIds, allProjectIds)">
            {{ projectIds.length === allProjectIds.length ? t('skills.bulk.none') : t('skills.bulk.all') }}
          </Button>
        </div>
        <div class="max-h-48 overflow-auto rounded-md border border-border/60 p-1">
          <label
            v-for="p in allocation.projects"
            :key="p.id"
            class="flex cursor-pointer items-center gap-2 rounded px-2 py-1 hover:bg-muted/50"
          >
            <input
              type="checkbox"
              class="accent-primary"
              :checked="projectIds.includes(p.id)"
              @change="projectIds = toggleId(projectIds, p.id, allProjectIds)"
            >
            <span class="truncate">{{ p.name }}</span>
          </label>
        </div>
      </section>

      <section class="flex flex-wrap items-center gap-3 md:col-span-2">
        <span class="font-medium">{{ t('skills.bulk.engines') }}</span>
        <label v-for="e in allEngines" :key="e" class="flex cursor-pointer items-center gap-1.5">
          <input
            type="checkbox"
            class="accent-primary"
            :checked="engines.includes(e)"
            @change="engines = toggleId(engines, e, allEngines)"
          >
          {{ engineLabel(e) }}
        </label>
        <SegmentedControl v-model="action" :options="actionOptions" :stretch="false" class="ml-auto" />
      </section>

      <section class="md:col-span-2">
        <Banner v-if="error" tone="destructive">{{ error }}</Banner>
        <p v-else-if="!plan" class="text-muted-foreground">{{ t('skills.bulk.previewHint') }}</p>
        <template v-else-if="summary">
          <p class="mb-2 font-medium">
            {{ t('skills.bulk.summary', summary) }}
          </p>
          <p v-if="sections.length === 0" class="text-muted-foreground">{{ t('skills.bulk.nothing') }}</p>
          <div v-for="sec in sections" :key="sec.projectId" class="mb-3 rounded-md border border-border/60">
            <h5 class="border-b border-border/60 px-3 py-1.5 font-medium">{{ sec.projectName }}</h5>
            <ul class="divide-y divide-border/40">
              <li v-for="c in sec.changes" :key="c.skill_id" class="px-3 py-1.5">
                <div class="flex items-center gap-2">
                  <span class="font-mono font-medium">{{ c.skill_name }}</span>
                  <span class="text-muted-foreground">{{ c.engines.map(engineLabel).join(' · ') }}</span>
                  <span v-if="c.files.length === 0" class="text-muted-foreground">— {{ t('skills.bulk.switchOnly') }}</span>
                </div>
                <div
                  v-for="f in c.files"
                  :key="f.path"
                  class="mt-0.5 flex items-start gap-1.5 break-all font-mono text-2xs"
                  :class="f.action === 'create' ? 'text-primary' : 'text-destructive'"
                  :title="t(`skills.bulk.file.${f.action}`)"
                >
                  <FilePlus v-if="f.action === 'create'" class="mt-px h-3 w-3 shrink-0" :stroke-width="1.75" />
                  <FileMinus v-else class="mt-px h-3 w-3 shrink-0" :stroke-width="1.75" />
                  {{ f.path }}
                </div>
              </li>
              <li
                v-for="s in sec.skipped"
                :key="`${s.skill_id}/${s.engine ?? ''}`"
                class="px-3 py-1.5 text-muted-foreground"
              >
                <span class="font-mono">{{ s.skill_name }}</span>
                <span v-if="s.engine"> · {{ engineLabel(s.engine) }}</span>
                — {{ t(`skills.bulk.skip.${s.reason}`) }}
                <div v-if="s.path" class="break-all font-mono text-2xs">{{ s.path }}</div>
              </li>
            </ul>
          </div>
        </template>
      </section>
    </div>

    <template #footer>
      <Button variant="ghost" @click="emit('close')">{{ t('common.cancel') }}</Button>
      <Button variant="outline" :disabled="!ready || planning || applying" @click="preview">
        <Loader2 v-if="planning" class="h-3.5 w-3.5 animate-spin" :stroke-width="1.75" />
        {{ t('skills.bulk.preview') }}
      </Button>
      <Button :disabled="!plan?.changes.length || applying" @click="apply">
        <Loader2 v-if="applying" class="h-3.5 w-3.5 animate-spin" :stroke-width="1.75" />
        {{ t('skills.bulk.apply', plan?.changes.length ?? 0) }}
      </Button>
    </template>
  </Modal>
</template>
