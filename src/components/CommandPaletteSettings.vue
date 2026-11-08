<script setup lang="ts">
// Settings → Command palette: turn groups on / off and order them, review the
// pinned and hidden items. Each part has its own "Restore defaults".
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowDown, ArrowUp, Command, Eye, EyeOff, Pin, PinOff, RotateCcw } from 'lucide-vue-next'
import { Button, Card, ToggleSwitch } from '@/components/ui'
import { IS_MAC } from '@/lib/appMenu'
import { errorText } from '@/lib/featureGate'
import {
  defaultPaletteConfig,
  moveGroup,
  resetPalettePart,
  setGroupEnabled,
  togglePinned,
  unhideItem,
  type PaletteConfig,
  type PaletteConfigPart,
} from '@/lib/commandPalette'
import { useCommandPaletteStore } from '@/stores/commandPalette'
import { usePaletteItems } from '@/composables/usePaletteItems'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const palette = useCommandPaletteStore()
const { items } = usePaletteItems()
const { toast } = useToast()

const config = computed(() => palette.config)
const shortcut = IS_MAC ? '⌘K' : 'Ctrl+K'

// An id whose item is gone (a deleted project, a session not loaded yet) still
// shows, by its id, so it can be unpinned or shown again.
const labelOf = computed(() => new Map(items.value.map((i) => [i.id, i.label])))

const isDefault = (part: PaletteConfigPart) =>
  JSON.stringify(config.value[part]) === JSON.stringify(defaultPaletteConfig()[part])

async function save(next: PaletteConfig) {
  try {
    await palette.save(next)
  } catch (e) {
    toast.error(errorText(e, t))
  }
}
</script>

<template>
  <Card body-class="p-4 space-y-6">
    <template #header>
      <Command class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('palette.settings.title') }}</span>
    </template>

    <p class="text-2xs text-muted-foreground">{{ t('palette.settings.help', { shortcut }) }}</p>

    <!-- Groups: on / off and order -->
    <section class="space-y-2">
      <div class="flex items-center gap-2">
        <h3 class="flex-1 text-xs font-semibold text-foreground">{{ t('palette.settings.groups') }}</h3>
        <Button variant="outline" size="xs" :disabled="isDefault('groups')" @click="save(resetPalettePart(config, 'groups'))">
          <RotateCcw class="h-3 w-3" :stroke-width="1.75" />
          {{ t('palette.settings.reset') }}
        </Button>
      </div>
      <div class="divide-y divide-border/60 rounded-lg border border-border/60">
        <div v-for="(g, i) in config.groups" :key="g.id" class="flex items-center gap-2 px-3 py-2">
          <span class="flex-1 truncate text-sm" :class="g.enabled ? 'text-foreground' : 'text-muted-foreground'">
            {{ t(`palette.group.${g.id}`) }}
          </span>
          <Button
            variant="ghost"
            size="icon-xs"
            :disabled="i === 0"
            :title="t('palette.settings.moveUp')"
            :aria-label="t('palette.settings.moveUp')"
            @click="save(moveGroup(config, g.id, -1))"
          >
            <ArrowUp class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            :disabled="i === config.groups.length - 1"
            :title="t('palette.settings.moveDown')"
            :aria-label="t('palette.settings.moveDown')"
            @click="save(moveGroup(config, g.id, 1))"
          >
            <ArrowDown class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
          <ToggleSwitch
            :checked="g.enabled"
            :label="t(`palette.group.${g.id}`)"
            @toggle="save(setGroupEnabled(config, g.id, !g.enabled))"
          />
        </div>
      </div>
    </section>

    <!-- Pinned items, in pin order -->
    <section class="space-y-2">
      <div class="flex items-center gap-2">
        <h3 class="flex-1 text-xs font-semibold text-foreground">{{ t('palette.settings.pinned') }}</h3>
        <Button variant="outline" size="xs" :disabled="isDefault('pinned')" @click="save(resetPalettePart(config, 'pinned'))">
          <RotateCcw class="h-3 w-3" :stroke-width="1.75" />
          {{ t('palette.settings.reset') }}
        </Button>
      </div>
      <p v-if="!config.pinned.length" class="flex items-center gap-1.5 text-2xs text-muted-foreground">
        <Pin class="h-3 w-3" :stroke-width="1.75" />
        {{ t('palette.settings.pinnedEmpty') }}
      </p>
      <div v-else class="divide-y divide-border/60 rounded-lg border border-border/60">
        <div v-for="id in config.pinned" :key="id" class="flex items-center gap-2 px-3 py-1.5">
          <span class="flex-1 truncate text-sm" :class="labelOf.has(id) ? 'text-foreground' : 'italic text-muted-foreground'">
            {{ labelOf.get(id) ?? id }}
          </span>
          <Button
            variant="ghost"
            size="icon-xs"
            :title="t('palette.unpin')"
            :aria-label="t('palette.unpin')"
            @click="save(togglePinned(config, id))"
          >
            <PinOff class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </div>
      </div>
    </section>

    <!-- Hidden items -->
    <section class="space-y-2">
      <div class="flex items-center gap-2">
        <h3 class="flex-1 text-xs font-semibold text-foreground">{{ t('palette.settings.hidden') }}</h3>
        <Button variant="outline" size="xs" :disabled="isDefault('hidden')" @click="save(resetPalettePart(config, 'hidden'))">
          <RotateCcw class="h-3 w-3" :stroke-width="1.75" />
          {{ t('palette.settings.reset') }}
        </Button>
      </div>
      <p v-if="!config.hidden.length" class="flex items-center gap-1.5 text-2xs text-muted-foreground">
        <EyeOff class="h-3 w-3" :stroke-width="1.75" />
        {{ t('palette.settings.hiddenEmpty') }}
      </p>
      <div v-else class="divide-y divide-border/60 rounded-lg border border-border/60">
        <div v-for="id in config.hidden" :key="id" class="flex items-center gap-2 px-3 py-1.5">
          <span class="flex-1 truncate text-sm" :class="labelOf.has(id) ? 'text-foreground' : 'italic text-muted-foreground'">
            {{ labelOf.get(id) ?? id }}
          </span>
          <Button
            variant="ghost"
            size="icon-xs"
            :title="t('palette.settings.unhide')"
            :aria-label="t('palette.settings.unhide')"
            @click="save(unhideItem(config, id))"
          >
            <Eye class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </div>
      </div>
    </section>
  </Card>
</template>
