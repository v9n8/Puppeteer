<script setup lang="ts">
// The Allocation tab of the Rules, Agents and Commands screens: one row per
// library item, one column per project, one chip per engine in each cell —
// the Skills matrix (skills/SkillAllocationMatrix) for the other libraries.
// Agents and commands have a Claude chip only. A rule shows Claude and Codex,
// but which of them it reaches is its target, so any of its chips switches the
// whole rule in that project. A file the repo commits to git, an engine the
// rule's target leaves out, and a rule a group keeps are shown, never switched.
import { computed, onActivated, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Lock, RotateCw, Search } from 'lucide-vue-next'
import { Banner, Button, Input } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { useProjectsStore } from '@/stores/projects'
import { useLibraryItemsStore } from '@/stores/libraryItems'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import { engineLabel } from '@/lib/skillRows'
import {
  ITEM_CHIP_STATES,
  buildItemMatrix,
  itemChipSwitchable,
  type AllocationKind,
  type ItemAllocation,
  type ItemChipState,
  type ItemMatrixCell,
  type ItemMatrixChip,
} from '@/lib/libraryAllocation'

const props = defineProps<{ kind: AllocationKind }>()
const emit = defineEmits<{ open: [itemId: string] }>()

const { t } = useI18n()
const { toast } = useToast()
const projectsStore = useProjectsStore()
// Through the store, so the screen's own per-project list stays current.
const itemsStore = props.kind === 'rule' ? null : useLibraryItemsStore(props.kind)

const allocation = ref<ItemAllocation | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const search = ref('')
/** `<item>/<project>` of the cell being switched. */
const busy = ref<string | null>(null)

const rows = computed(() => (allocation.value ? buildItemMatrix(allocation.value, search.value) : []))
// `off_target` only ever shows for rules.
const legend = computed(() => ITEM_CHIP_STATES.filter((s) => s !== 'off_target' || props.kind === 'rule'))

const chipClass: Record<ItemChipState, string> = {
  installed: 'bg-primary/12 text-primary ring-primary/30',
  drifted: 'bg-warning/15 text-warning ring-warning/30',
  git_managed: 'bg-secondary text-muted-foreground ring-border cursor-not-allowed',
  not_installed: 'bg-transparent text-muted-foreground/60 ring-border hover:text-foreground',
  off_target: 'bg-transparent text-muted-foreground/40 ring-border/50 cursor-not-allowed',
}

async function load() {
  loading.value = true
  error.value = null
  try {
    allocation.value = await invoke<ItemAllocation>('list_library_allocation', { kind: props.kind })
  } catch (e) {
    error.value = errorText(e, t)
  } finally {
    loading.value = false
  }
}

onMounted(load)
// Back on the cached screen: the folders may have changed meanwhile. A load
// already running (the first mount) covers it.
onActivated(() => {
  if (!loading.value) void load()
})

function chipTitle(projectName: string, cell: ItemMatrixCell, chip: ItemMatrixChip): string {
  const lines = [`${projectName} · ${engineLabel(chip.engine)}: ${t(`libraryAllocation.state.${chip.state}`)}`]
  lines.push(cell.held && chip.enabled ? t('libraryAllocation.held') : t(`libraryAllocation.hint.${chip.state}`))
  return lines.join('\n')
}

async function toggle(itemId: string, cell: ItemMatrixCell, chip: ItemMatrixChip) {
  if (!itemChipSwitchable(cell, chip) || busy.value) return
  busy.value = `${itemId}/${cell.projectId}`
  try {
    if (itemsStore) {
      if (cell.applied) await itemsStore.removeFromProject(cell.projectId, itemId)
      else await itemsStore.applyToProject(cell.projectId, itemId)
    } else if (cell.applied) {
      await projectsStore.removeRuleFromProject(cell.projectId, itemId)
    } else {
      await projectsStore.applyRule(cell.projectId, itemId)
    }
  } catch (e) {
    toast.error(errorText(e, t))
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
        <Input v-model="search" :placeholder="t('libraryAllocation.searchPlaceholder')" class="pl-7" />
      </div>
      <Button
        variant="outline"
        size="icon"
        :disabled="loading"
        :title="t('libraryAllocation.refresh')"
        :aria-label="t('libraryAllocation.refresh')"
        @click="load"
      >
        <RotateCw class="h-3.5 w-3.5" :stroke-width="1.75" :class="{ 'animate-spin': loading }" />
      </Button>
      <span v-if="kind === 'rule'" class="text-2xs text-muted-foreground">{{ t('libraryAllocation.ruleNote') }}</span>
      <div class="ml-auto flex flex-wrap items-center gap-1.5 text-2xs text-muted-foreground">
        <span
          v-for="state in legend"
          :key="state"
          class="inline-flex items-center gap-1 rounded-full px-1.5 py-px ring-1 ring-inset"
          :class="chipClass[state]"
          :title="t(`libraryAllocation.hint.${state}`)"
        >
          <Lock v-if="state === 'git_managed'" class="h-2.5 w-2.5" :stroke-width="2" />
          {{ t(`libraryAllocation.state.${state}`) }}
        </span>
      </div>
    </div>

    <div class="min-h-0 flex-1 overflow-auto">
      <div v-if="error" class="p-4">
        <Banner tone="destructive">{{ error }}</Banner>
      </div>
      <p v-else-if="allocation && allocation.projects.length === 0" class="p-6 text-center text-xs text-muted-foreground">
        {{ t('libraryAllocation.noProjects') }}
      </p>
      <p v-else-if="allocation && rows.length === 0" class="p-6 text-center text-xs text-muted-foreground">
        {{ allocation.items.length ? t('libraryAllocation.noMatch') : t('libraryAllocation.noItems') }}
      </p>
      <table v-else-if="allocation" class="border-separate border-spacing-0 text-xs">
        <thead>
          <tr>
            <th class="sticky left-0 top-0 z-20 border-b border-r border-border/60 bg-background px-3 py-2 text-left font-medium text-muted-foreground">
              {{ t(`libraryAllocation.item.${kind}`) }}
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
          <tr v-for="row in rows" :key="row.item.id" class="hover:bg-muted/40">
            <th class="sticky left-0 z-10 border-b border-r border-border/60 bg-background px-3 py-1.5 text-left font-normal">
              <button
                type="button"
                class="max-w-48 truncate font-mono font-medium hover:text-primary hover:underline"
                :title="row.item.description || row.item.name"
                @click="emit('open', row.item.id)"
              >
                {{ row.item.name }}
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
                  :disabled="!itemChipSwitchable(cell, chip) || busy !== null"
                  :aria-pressed="chip.enabled"
                  :title="chipTitle(allocation.projects[i].name, cell, chip)"
                  @click="toggle(row.item.id, cell, chip)"
                >
                  <Lock v-if="chip.state === 'git_managed' || (cell.held && chip.enabled)" class="h-2.5 w-2.5" :stroke-width="2" />
                  <RotateCw
                    v-else-if="busy === `${row.item.id}/${cell.projectId}`"
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
