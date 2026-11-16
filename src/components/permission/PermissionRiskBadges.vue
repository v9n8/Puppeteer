<script setup lang="ts">
// Read / Write / Delete / Network badges for a permission request. Icon + word,
// never colour alone.
import { useI18n } from 'vue-i18n'
import { Eye, Pencil, Trash2, Globe } from 'lucide-vue-next'
import type { PermissionRisk } from '@/lib/permissionImpact'

defineProps<{ risks: PermissionRisk[] }>()
const { t } = useI18n()

const ICONS = { read: Eye, write: Pencil, delete: Trash2, network: Globe }
const TONES: Record<PermissionRisk, string> = {
  read: 'border-border text-foreground/70',
  write: 'border-warning/40 bg-warning/15 text-warning',
  network: 'border-info/40 bg-info/15 text-info',
  delete: 'border-destructive/40 bg-destructive/15 text-destructive',
}
</script>

<template>
  <span v-for="r in risks" :key="r" class="inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-2xs font-medium" :class="TONES[r]">
    <component :is="ICONS[r]" class="h-3 w-3 shrink-0" :stroke-width="1.75" />
    {{ t(`permission.risk.${r}`) }}
  </span>
</template>
