<script setup lang="ts">
// "All flags ▾" at the top of the History column (V3-19): filters the list by the
// Session's flag, with the number of Sessions behind every choice. Its state lives
// in useRunHistoryList (per project); the number in "History · N" follows it.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { useFeaturesStore } from '@/stores/features'
import { Check, ChevronDown, Filter } from 'lucide-vue-next'
import { DropdownMenu, DropdownItem, DropdownSeparator } from '@/components/ui'
import { flagById, flagLabel } from '@/lib/sessionFlags'
import { swatchClass } from '@/lib/projectAppearance'
import { useFlagTargets } from './useFlagTargets'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const { projectId, projectFlags, flagFilter, flagCounts, flagOptions } = useRunSession()
const { canOpenProjectFlags, openProjectFlags } = useFlagTargets()

const features = useFeaturesStore()
const router = useRouter()
const boardOn = computed(() => features.isOn('sessions.board'))
// The board opens on the Project whose History this is.
function openBoard() {
  router.push({ path: '/board', query: { project: projectId.value } }).catch(() => {})
}

const selected = computed(() => flagById(projectFlags.value, flagFilter.value))
const buttonLabel = computed(() => {
  if (flagFilter.value === 'none') return t('sessionFlags.filter.none')
  return selected.value ? flagLabel(selected.value, t) : t('sessionFlags.filter.all')
})
const groups = computed(() => [
  { key: 'shared', title: t('sessionFlags.menu.shared'), options: flagOptions.value.shared },
  { key: 'project', title: t('sessionFlags.menu.project'), options: flagOptions.value.project },
])
</script>

<template>
  <DropdownMenu align="left" class="min-w-0">
    <template #trigger>
      <button
        type="button"
        class="inline-flex h-7 min-w-0 max-w-[9rem] cursor-pointer items-center gap-1.5 rounded px-1.5 text-2xs normal-case tracking-normal transition-colors hover:bg-accent focus:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        :class="flagFilter === 'all' ? 'text-muted-foreground' : 'bg-accent/60 text-foreground'"
        :title="t('sessionFlags.filter.tooltip', { name: buttonLabel })"
        :aria-label="t('sessionFlags.filter.tooltip', { name: buttonLabel })"
        aria-haspopup="menu"
      >
        <span v-if="selected" class="h-2 w-2 shrink-0 rounded-full" :class="swatchClass(selected.color)" aria-hidden="true" />
        <Filter v-else class="h-3 w-3 shrink-0" :stroke-width="1.75" />
        <span class="min-w-0 truncate">{{ buttonLabel }}</span>
        <ChevronDown class="h-3 w-3 shrink-0" :stroke-width="1.75" />
      </button>
    </template>
    <div class="min-w-[14rem]">
      <div class="px-2.5 pb-0.5 pt-1 text-2xs font-medium uppercase tracking-wider text-muted-foreground/70">
        {{ t('sessionFlags.filter.title') }}
      </div>
      <DropdownItem @click="flagFilter = 'all'">
        <span class="min-w-0 flex-1 truncate">{{ t('sessionFlags.filter.all') }}</span>
        <span class="font-mono text-2xs text-muted-foreground">{{ flagCounts.total }}</span>
        <Check class="h-3.5 w-3.5 shrink-0 text-primary" :class="{ invisible: flagFilter !== 'all' }" :stroke-width="2" />
      </DropdownItem>
      <template v-for="group in groups" :key="group.key">
        <template v-if="group.options.length">
          <div class="px-2.5 pb-0.5 pt-1 text-2xs font-medium uppercase tracking-wider text-muted-foreground/70">
            {{ group.title }}
          </div>
          <DropdownItem v-for="{ flag, count } in group.options" :key="flag.id" @click="flagFilter = flag.id">
            <span class="h-2 w-2 shrink-0 rounded-full" :class="swatchClass(flag.color)" aria-hidden="true" />
            <span class="min-w-0 flex-1 truncate">{{ flagLabel(flag, t) }}</span>
            <span class="font-mono text-2xs text-muted-foreground">{{ count }}</span>
            <Check class="h-3.5 w-3.5 shrink-0 text-primary" :class="{ invisible: flagFilter !== flag.id }" :stroke-width="2" />
          </DropdownItem>
        </template>
      </template>
      <DropdownSeparator />
      <DropdownItem @click="flagFilter = 'none'">
        <span class="min-w-0 flex-1 truncate">{{ t('sessionFlags.filter.none') }}</span>
        <span class="font-mono text-2xs text-muted-foreground">{{ flagCounts.none }}</span>
        <Check class="h-3.5 w-3.5 shrink-0 text-primary" :class="{ invisible: flagFilter !== 'none' }" :stroke-width="2" />
      </DropdownItem>
      <template v-if="canOpenProjectFlags || boardOn">
        <DropdownSeparator />
        <DropdownItem v-if="boardOn" @click="openBoard">{{ t('board.viewAsBoard') }}</DropdownItem>
        <DropdownItem v-if="canOpenProjectFlags" @click="openProjectFlags(projectId)">
          {{ t('sessionFlags.menu.manage') }}
        </DropdownItem>
      </template>
    </div>
  </DropdownMenu>
</template>
