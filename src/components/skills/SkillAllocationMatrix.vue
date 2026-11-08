<script setup lang="ts">
// The Skills screen's Allocation tab: one row per skill, one column per
// project, one chip per engine in each cell. A chip switches that skill on or
// off for that engine in that project only; a skill the repo commits to git is
// shown but never switched.
import { computed, onActivated, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Lock, RotateCw, Search } from 'lucide-vue-next'
import { useSkillsStore } from '@/stores/skills'
import { useToast } from '@/composables/useToast'
import { Banner, Button, Input } from '@/components/ui'
import {
  CHIP_STATES,
  buildAllocationMatrix,
  chipSwitchable,
  engineLabel,
  type ChipState,
  type MatrixChip,
  type SkillAllocation,
} from '@/lib/skillRows'

const emit = defineEmits<{ open: [skillId: string] }>()

const { t } = useI18n()
const { toast } = useToast()
const store = useSkillsStore()

const allocation = ref<SkillAllocation | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const search = ref('')
/** `<skill>/<project>/<engine>` of the chip being switched. */
const busy = ref<string | null>(null)

const rows = computed(() => (allocation.value ? buildAllocationMatrix(store.skills, allocation.value, search.value) : []))

const chipClass: Record<ChipState, string> = {
  installed: 'bg-primary/12 text-primary ring-primary/30',
  drifted: 'bg-warning/15 text-warning ring-warning/30',
  git_managed: 'bg-secondary text-muted-foreground ring-border cursor-not-allowed',
  shadowed: 'bg-violet-500/15 text-violet-700 ring-violet-500/30 line-through dark:text-violet-300',
  not_installed: 'bg-transparent text-muted-foreground/60 ring-border hover:text-foreground',
}

async function load() {
  loading.value = true
  error.value = null
  try {
    allocation.value = await store.getAllocation()
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

onMounted(load)
// Back on the cached Skills screen: the folders may have changed meanwhile. A
// load already running (the first mount) covers it.
onActivated(() => {
  if (!loading.value) void load()
})

function chipTitle(projectName: string, chip: MatrixChip): string {
  return `${projectName} · ${engineLabel(chip.engine)}: ${t(`skills.allocation.state.${chip.state}`)}\n${t(`skills.allocation.hint.${chip.state}`)}`
}

async function toggle(skillId: string, projectId: string, chip: MatrixChip) {
  if (!chipSwitchable(chip) || busy.value) return
  busy.value = `${skillId}/${projectId}/${chip.engine}`
  try {
    await store.setSkillEngine(projectId, skillId, chip.engine, !chip.enabled)
  } catch (e) {
    toast.error(String(e))
  } finally {
    // The folder decides what the chip shows, so read it back either way.
    await load()
    busy.value = null
  }
}
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex flex-wrap items-center gap-2 border-b border-border/60 px-4 py-2">
      <div class="relative w-56">
        <Search class="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" :stroke-width="1.75" />
        <Input v-model="search" :placeholder="t('skills.detail.searchPlaceholder')" class="pl-7" />
      </div>
      <Button
        variant="outline"
        size="icon"
        :disabled="loading"
        :title="t('skills.allocation.refresh')"
        :aria-label="t('skills.allocation.refresh')"
        @click="load"
      >
        <RotateCw class="h-3.5 w-3.5" :stroke-width="1.75" :class="{ 'animate-spin': loading }" />
      </Button>
      <div class="ml-auto flex flex-wrap items-center gap-1.5 text-2xs text-muted-foreground">
        <span
          v-for="state in CHIP_STATES"
          :key="state"
          class="inline-flex items-center gap-1 rounded-full px-1.5 py-px ring-1 ring-inset"
          :class="chipClass[state]"
          :title="t(`skills.allocation.hint.${state}`)"
        >
          <Lock v-if="state === 'git_managed'" class="h-2.5 w-2.5" :stroke-width="2" />
          {{ t(`skills.allocation.state.${state}`) }}
        </span>
      </div>
    </div>

    <div class="min-h-0 flex-1 overflow-auto">
      <div v-if="error" class="p-4">
        <Banner tone="destructive">{{ error }}</Banner>
      </div>
      <p v-else-if="allocation && allocation.projects.length === 0" class="p-6 text-center text-xs text-muted-foreground">
        {{ t('skills.allocation.noProjects') }}
      </p>
      <p v-else-if="allocation && rows.length === 0" class="p-6 text-center text-xs text-muted-foreground">
        {{ store.skills.length ? t('skills.allocation.noMatch') : t('skills.list.emptyHint') }}
      </p>
      <table v-else-if="allocation" class="border-separate border-spacing-0 text-xs">
        <thead>
          <tr>
            <th class="sticky left-0 top-0 z-20 border-b border-r border-border/60 bg-background px-3 py-2 text-left font-medium text-muted-foreground">
              {{ t('skills.allocation.skill') }}
            </th>
            <th
              v-for="p in allocation.projects"
              :key="p.id"
              class="sticky top-0 z-10 max-w-40 truncate border-b border-border/60 bg-background px-3 py-2 text-left font-medium"
              :title="p.name"
            >
              {{ p.name }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.skill.id" class="hover:bg-muted/40">
            <th class="sticky left-0 z-10 border-b border-r border-border/60 bg-background px-3 py-1.5 text-left font-normal">
              <button
                type="button"
                class="max-w-48 truncate font-mono font-medium hover:text-primary hover:underline"
                :title="row.skill.description || row.skill.name"
                @click="emit('open', row.skill.id)"
              >
                {{ row.skill.name }}
              </button>
            </th>
            <td
              v-for="(cell, i) in row.cells"
              :key="cell.projectId"
              class="border-b border-border/40 px-3 py-1.5"
            >
              <div class="flex gap-1">
                <button
                  v-for="chip in cell.chips"
                  :key="chip.engine"
                  type="button"
                  class="inline-flex items-center gap-1 rounded-full px-1.5 py-px text-2xs font-medium ring-1 ring-inset transition-colors disabled:opacity-60"
                  :class="chipClass[chip.state]"
                  :disabled="!chipSwitchable(chip) || busy !== null"
                  :aria-pressed="chip.enabled"
                  :title="chipTitle(allocation.projects[i].name, chip)"
                  @click="toggle(row.skill.id, cell.projectId, chip)"
                >
                  <Lock v-if="chip.state === 'git_managed'" class="h-2.5 w-2.5" :stroke-width="2" />
                  <RotateCw
                    v-else-if="busy === `${row.skill.id}/${cell.projectId}/${chip.engine}`"
                    class="h-2.5 w-2.5 animate-spin"
                    :stroke-width="2"
                  />
                  {{ engineLabel(chip.engine) }}
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
