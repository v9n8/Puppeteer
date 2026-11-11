<script setup lang="ts">
// Settings › General › Interface font: one tile per family, each with a sample
// drawn in that font, so the choice is made by looking rather than by name.
import { useI18n } from 'vue-i18n'
import { Check, Pencil } from 'lucide-vue-next'
import { FONT_FAMILIES, familyStack, type FontFamilyId } from '@/lib/appFonts'

const model = defineModel<FontFamilyId>({ required: true })

const { t } = useI18n()

const labelKey = (f: FontFamilyId) => `settings.general.font${f.charAt(0).toUpperCase()}${f.slice(1)}`
</script>

<template>
  <div class="grid grid-cols-2 gap-2 min-[1100px]:grid-cols-3">
    <button
      v-for="f in FONT_FAMILIES"
      :key="f"
      type="button"
      class="flex flex-col gap-0.5 rounded-lg border bg-card px-3 py-2.5 text-left transition-colors cursor-pointer"
      :class="model === f ? 'border-primary ring-1 ring-primary' : 'border-border hover:border-input'"
      :aria-pressed="model === f"
      @click="model = f"
    >
      <span class="flex h-8 items-center justify-between gap-2 text-xl text-foreground" aria-hidden="true">
        <span v-if="familyStack(f)" :style="{ fontFamily: familyStack(f)! }">Aa Ăn</span>
        <Pencil v-else class="h-5 w-5 text-muted-foreground" :stroke-width="1.75" />
        <Check v-if="model === f" class="h-4 w-4 shrink-0 text-primary" :stroke-width="2" />
      </span>
      <span class="text-xs font-medium">{{ t(labelKey(f)) }}</span>
      <span class="text-xs text-muted-foreground text-pretty">{{ t(`${labelKey(f)}Hint`) }}</span>
    </button>
  </div>
</template>
