<script setup lang="ts">
// The open Session's flag, right after its name in the header (V3-19): a dot and
// the flag's name, or a dashed "Add flag" while it has none. Click opens the same
// menu as History's "Flag ▸" to change or remove it.
import { computed, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronDown, Flag } from 'lucide-vue-next'
import { DropdownMenu } from '@/components/ui'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import { flagById, flagLabel } from '@/lib/sessionFlags'
import { swatchClass } from '@/lib/projectAppearance'
import FlagMenuItems from './FlagMenuItems.vue'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const { currentRun, projectId } = useRunSession()
const flagsStore = useSessionFlagsStore()

const flag = computed(() => flagById(flagsStore.flagsOf(projectId.value), currentRun.value?.flag_id))
const name = computed(() => (flag.value ? flagLabel(flag.value, t) : ''))

function load() {
  flagsStore.load(projectId.value).catch((e) => console.warn('[sessionFlags] load failed', e))
}
onMounted(load)
watch(projectId, load)
</script>

<template>
  <DropdownMenu v-if="currentRun" align="left" class="shrink-0">
    <template #trigger>
      <button
        type="button"
        class="inline-flex h-7 max-w-[132px] min-w-0 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs transition-colors focus:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        :class="flag
          ? 'border border-border bg-card text-foreground hover:bg-accent'
          : 'border border-dashed border-border text-muted-foreground hover:bg-accent hover:text-foreground'"
        :title="flag ? t('sessionFlags.chip.tooltip', { name }) : t('sessionFlags.chip.tooltipNone')"
        :aria-label="flag ? t('sessionFlags.chip.tooltip', { name }) : t('sessionFlags.chip.tooltipNone')"
        aria-haspopup="menu"
      >
        <span v-if="flag" class="h-2 w-2 shrink-0 rounded-full" :class="swatchClass(flag.color)" aria-hidden="true" />
        <Flag v-else class="h-3 w-3 shrink-0" :stroke-width="1.75" />
        <span class="min-w-0 truncate">{{ flag ? name : t('sessionFlags.chip.add') }}</span>
        <ChevronDown class="h-3 w-3 shrink-0 text-muted-foreground" :stroke-width="1.75" />
      </button>
    </template>
    <FlagMenuItems :run-id="currentRun.id" :project-id="projectId" :flag-id="currentRun.flag_id" />
  </DropdownMenu>
</template>
