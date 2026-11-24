<script setup lang="ts">
// The "Set flag" menu of one Session (V3-19), as DropdownItems: "No flag", the
// shared flags, the project's own (flags the project hides are left out), then
// "Manage flags", which opens this project's Flag tab. Groups are told apart by
// separators, not headings (UR-02). Used in the header chip's menu and in the
// "Flag ▸" submenu of a History row / the header ⋯. The caller's DropdownMenu
// closes after a click.
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check } from 'lucide-vue-next'
import { DropdownItem, DropdownSeparator } from '@/components/ui'
import { useRunsStore } from '@/stores/runs'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import { useToast } from '@/composables/useToast'
import { flagLabel, flagMenuGroups } from '@/lib/sessionFlags'
import { swatchClass } from '@/lib/projectAppearance'
import { useFlagTargets } from './useFlagTargets'

const props = defineProps<{
  runId: string
  projectId: string
  flagId: string | null | undefined
  // Tells a screen with its own copy of the run (the board) that the flag was saved. A prop,
  // not an emit: the menu has closed and this component is unmounted when the save returns,
  // and Vue drops an emit from an unmounted component.
  onChanged?: (flagId: string | null) => void
}>()

const { t } = useI18n()
const runsStore = useRunsStore()
const flagsStore = useSessionFlagsStore()
const { toast } = useToast()
const { canOpenProjectFlags, openProjectFlags } = useFlagTargets()

const groups = computed(() => flagMenuGroups(flagsStore.flagsOf(props.projectId)))

onMounted(() => {
  if (!flagsStore.byScope.has(props.projectId)) {
    flagsStore.load(props.projectId).catch((e) => console.warn('[sessionFlags] load failed', e))
  }
})

async function choose(flagId: string | null) {
  if (flagId === (props.flagId ?? null)) return
  try {
    await runsStore.setRunFlag(props.runId, flagId)
    props.onChanged?.(flagId)
  } catch (e) {
    // The flag may have been hidden or deleted in another window: show why and re-read the list.
    toast.error(String(e))
    flagsStore.load(props.projectId).catch((err) => console.warn('[sessionFlags] reload failed', err))
  }
}
</script>

<template>
  <div role="group">
    <DropdownItem @click="choose(null)">
      <span class="min-w-0 flex-1 truncate">{{ t('sessionFlags.menu.none') }}</span>
      <Check v-if="!flagId" class="h-3.5 w-3.5 shrink-0 text-primary" :stroke-width="2" />
    </DropdownItem>
    <template v-for="group in [groups.shared, groups.project]" :key="group === groups.shared ? 'shared' : 'project'">
      <template v-if="group.length">
        <DropdownSeparator />
        <DropdownItem v-for="flag in group" :key="flag.id" @click="choose(flag.id)">
          <span class="h-2 w-2 shrink-0 rounded-full" :class="swatchClass(flag.color)" aria-hidden="true" />
          <span class="min-w-0 flex-1 truncate">{{ flagLabel(flag) }}</span>
          <Check v-if="flag.id === flagId" class="h-3.5 w-3.5 shrink-0 text-primary" :stroke-width="2" />
        </DropdownItem>
      </template>
    </template>
    <template v-if="canOpenProjectFlags">
      <DropdownSeparator />
      <DropdownItem @click="openProjectFlags(projectId)">
        {{ t('sessionFlags.menu.manage') }}
      </DropdownItem>
    </template>
  </div>
</template>
