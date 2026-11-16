<script setup lang="ts">
// The "Set flag" menu of one Session (V3-19), as DropdownItems: shared flags,
// then the project's own (flags the project hides are left out), "Remove flag",
// "Manage flags…". Used in the header chip's menu and in the "Flag ▸" submenu of
// a History row / the header ⋯. The caller's DropdownMenu closes after a click.
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

const props = defineProps<{ runId: string; projectId: string; flagId: string | null | undefined }>()

const { t } = useI18n()
const runsStore = useRunsStore()
const flagsStore = useSessionFlagsStore()
const { toast } = useToast()
const { canManage, openManage } = useFlagTargets()

const groups = computed(() => flagMenuGroups(flagsStore.flagsOf(props.projectId)))
const hasUsable = computed(() => groups.value.shared.length + groups.value.project.length > 0)

onMounted(() => {
  if (!flagsStore.byScope.has(props.projectId)) {
    flagsStore.load(props.projectId).catch((e) => console.warn('[sessionFlags] load failed', e))
  }
})

async function choose(flagId: string | null) {
  if (flagId === (props.flagId ?? null)) return
  try {
    await runsStore.setRunFlag(props.runId, flagId)
  } catch (e) {
    // The flag may have been hidden or deleted in another window: show why and re-read the list.
    toast.error(String(e))
    flagsStore.load(props.projectId).catch((err) => console.warn('[sessionFlags] reload failed', err))
  }
}
</script>

<template>
  <div role="group">
    <template v-for="group in [
      { key: 'shared', title: t('sessionFlags.menu.shared'), flags: groups.shared },
      { key: 'project', title: t('sessionFlags.menu.project'), flags: groups.project },
    ]" :key="group.key">
      <template v-if="group.flags.length">
        <div class="px-2.5 pb-0.5 pt-1 text-2xs font-medium uppercase tracking-wider text-muted-foreground/70">
          {{ group.title }}
        </div>
        <DropdownItem v-for="flag in group.flags" :key="flag.id" @click="choose(flag.id)">
          <span class="h-2 w-2 shrink-0 rounded-full" :class="swatchClass(flag.color)" aria-hidden="true" />
          <span class="min-w-0 flex-1 truncate">{{ flagLabel(flag, t) }}</span>
          <Check v-if="flag.id === flagId" class="h-3.5 w-3.5 shrink-0 text-primary" :stroke-width="2" />
        </DropdownItem>
      </template>
    </template>
    <DropdownSeparator v-if="hasUsable" />
    <DropdownItem v-if="flagId || hasUsable" :disabled="!flagId" @click="choose(null)">
      {{ t('sessionFlags.menu.remove') }}
    </DropdownItem>
    <DropdownItem v-if="canManage" @click="openManage()">
      {{ t('sessionFlags.menu.manage') }}
    </DropdownItem>
  </div>
</template>
