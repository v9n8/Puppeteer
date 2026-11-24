<script setup lang="ts">
// One shared flag in Project › Flag (V3-21): the switch says whether this project's
// Sessions can be given it. Turning it off asks first (the Sessions that have it keep
// it); turning it on is immediate.
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Badge, ToggleSwitch } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import { sharedFlagSubline } from '@/lib/flagEditor'
import { swatchClass } from '@/lib/projectAppearance'
import { flagLabel, type SessionFlag } from '@/lib/sessionFlags'

const props = defineProps<{ flag: SessionFlag; projectId: string; projectName: string }>()

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const store = useSessionFlagsStore()
const busy = ref(false)

async function toggle() {
  if (busy.value) return
  const name = flagLabel(props.flag)
  if (!props.flag.hidden) {
    const ok = await confirm({
      title: t('sessionFlags.project.hideTitle', { name }),
      message: t(props.flag.run_count > 0 ? 'sessionFlags.project.hideMessage' : 'sessionFlags.project.hideMessageNone', {
        project: props.projectName,
        n: props.flag.run_count,
      }),
      confirmLabel: t('sessionFlags.project.hideConfirm'),
      variant: 'primary',
    })
    if (!ok) return
  }
  busy.value = true
  try {
    await store.setHidden(props.projectId, props.flag.id, !props.flag.hidden)
  } catch (e) {
    toast.error(String(e))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="flex items-center gap-3 border-b border-border/50 px-4 py-2.5 last:border-b-0">
    <span class="h-3 w-3 shrink-0 rounded-full" :class="swatchClass(flag.color)" />
    <div class="min-w-0 flex-1">
      <div class="flex min-w-0 items-center gap-2">
        <span class="truncate text-sm font-semibold text-foreground" :title="flagLabel(flag)">{{ flagLabel(flag) }}</span>
        <Badge v-if="flag.hidden" class="shrink-0">{{ t('sessionFlags.project.badgeHidden') }}</Badge>
      </div>
      <p class="mt-0.5 text-xs text-muted-foreground">{{ sharedFlagSubline(flag, t) }}</p>
    </div>
    <span class="hidden shrink-0 text-xs text-muted-foreground sm:inline">{{ t('sessionFlags.project.useInProject') }}</span>
    <ToggleSwitch
      :checked="!flag.hidden"
      :loading="busy"
      :label="t(flag.hidden ? 'sessionFlags.project.toggleUse' : 'sessionFlags.project.toggleHide', { name: flagLabel(flag) })"
      @toggle="toggle"
    />
  </div>
</template>
