<script setup lang="ts">
// The composer toolbar's always-visible chips (UX-26): account · model with its
// Model | Account menu (UX-47), permission mode, effort (only when the next
// turn's engine + model take one) and response style. State and defaults:
// useComposerChips; the menu's rows: useRunComposer.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, ChevronDown, Gauge, MessageSquareText, Sparkles, UserCircle } from 'lucide-vue-next'
import AppSelect from '@/components/AppSelect.vue'
import { Button, SegmentedControl, type SegmentedOption } from '@/components/ui'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  permissionMode, modeValue, modeOptions, modeChipLabel, modeChipIcon, modeChipTitle, effortPick, effortValue, effortLevelsNow, effortOptions,
  effortChipLabel, effortChipTitle, stylePick, styleValue, styleOptions, styleChipLabel, styleChipTitle,
  accountMenuOpen, accountMenuRoot, accountTab, toggleAccountMenu, closeAccountMenu, accountUsage,
  accountModelLabel, accountChoices, selectedAccountId, accountSelectDisabled, handleAccountChange,
  modelOptions, modelOverride, onModelSelect, currentStatus,
} = useRunSession()

const tabs = computed<SegmentedOption<'model' | 'account'>[]>(() => [
  { value: 'model', label: t('run.accountMenu.tabModel'), icon: Sparkles },
  { value: 'account', label: t('run.accountMenu.tabAccount'), icon: UserCircle },
])
const accountChipTitle = computed(() => t('run.accountMenu.chipTitle', { value: accountModelLabel.value }))

function pickModel(value: string) {
  closeAccountMenu()
  onModelSelect(value)
}
function pickAccount(value: string) {
  closeAccountMenu()
  void handleAccountChange(value)
}
</script>

<template>
  <div ref="accountMenuRoot" class="relative inline-flex min-w-0" @keydown.esc.stop="closeAccountMenu">
    <Button
      variant="ghost"
      size="xs"
      class="min-w-0"
      :title="accountChipTitle"
      :aria-label="accountChipTitle"
      aria-haspopup="dialog"
      :aria-expanded="accountMenuOpen"
      @click="toggleAccountMenu"
    >
      <UserCircle class="h-3 w-3 shrink-0" :stroke-width="1.75" />
      <span class="max-w-[14rem] truncate text-foreground" :title="accountChipTitle">{{ accountModelLabel }}</span>
      <ChevronDown
        class="h-3 w-3 shrink-0 transition-transform duration-200"
        :class="{ 'rotate-180': accountMenuOpen }"
        :stroke-width="2"
      />
    </Button>
    <transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 translate-y-1"
      leave-active-class="transition duration-100 ease-in"
      leave-to-class="opacity-0 translate-y-1"
    >
      <div
        v-if="accountMenuOpen"
        role="dialog"
        class="absolute bottom-full left-0 z-30 mb-2 w-80 max-w-[calc(100vw-2rem)] rounded-md border border-border bg-popover p-2 shadow-lg shadow-black/20"
        :aria-label="t('run.accountMenu.title')"
      >
        <SegmentedControl v-model="accountTab" :options="tabs" class="mb-2" />

        <template v-if="accountTab === 'model'">
          <ul class="max-h-72 overflow-y-auto text-xs">
            <template v-for="o in modelOptions" :key="o.value">
              <li
                v-if="o.header"
                class="px-2 pt-2 pb-1 text-2xs font-medium uppercase tracking-wide text-muted-foreground select-none"
              >{{ o.label }}</li>
              <li v-else>
                <button
                  type="button"
                  class="flex w-full cursor-pointer items-start gap-2 rounded px-2 py-1.5 text-left hover:bg-muted disabled:cursor-default disabled:opacity-50 disabled:hover:bg-transparent"
                  :disabled="currentStatus === 'running'"
                  @click="pickModel(o.value)"
                >
                  <Check
                    class="mt-0.5 h-3 w-3 shrink-0 text-primary"
                    :class="o.value === modelOverride ? 'opacity-100' : 'opacity-0'"
                    :stroke-width="2.5"
                  />
                  <span class="flex min-w-0 flex-col">
                    <span class="truncate" :title="o.label">{{ o.label }}</span>
                    <span
                      v-if="o.description"
                      class="truncate text-2xs leading-tight text-muted-foreground"
                      :title="o.description"
                    >{{ o.description }}</span>
                  </span>
                </button>
              </li>
            </template>
          </ul>
          <p v-if="currentStatus === 'running'" class="mt-1 px-2 text-2xs text-muted-foreground">
            {{ t('run.accountMenu.modelRunning') }}
          </p>
        </template>

        <template v-else>
          <ul class="max-h-72 overflow-y-auto text-xs">
            <li v-for="a in accountChoices" :key="a.value ?? a.label">
              <component
                :is="a.value === null ? 'div' : 'button'"
                :type="a.value === null ? undefined : 'button'"
                class="flex w-full items-start gap-2 rounded px-2 py-1.5 text-left"
                :class="a.value === null ? '' : 'cursor-pointer hover:bg-muted disabled:cursor-default disabled:opacity-50 disabled:hover:bg-transparent'"
                :disabled="a.value === null ? undefined : accountSelectDisabled"
                @click="a.value !== null && pickAccount(a.value)"
              >
                <Check
                  class="mt-0.5 h-3 w-3 shrink-0 text-primary"
                  :class="a.value === null || a.value === selectedAccountId ? 'opacity-100' : 'opacity-0'"
                  :stroke-width="2.5"
                />
                <span class="flex min-w-0 flex-1 flex-col">
                  <span class="truncate" :title="a.label">{{ a.label }}</span>
                  <span
                    v-if="a.detail"
                    class="truncate text-2xs leading-tight text-muted-foreground"
                    :title="a.detail"
                  >{{ a.detail }}</span>
                </span>
                <span class="flex shrink-0 gap-3">
                  <span
                    v-for="u in accountUsage(a.usageKey)"
                    :key="u.key"
                    class="flex flex-col items-end"
                    :title="u.reset ? `${u.title} · ${u.reset}` : u.title"
                  >
                    <span class="tabular-nums">
                      <span class="text-muted-foreground">{{ u.label }}</span>
                      <span class="ml-1 font-medium" :class="u.color">{{ u.percent }}%</span>
                    </span>
                    <span v-if="u.reset" class="text-2xs leading-tight text-muted-foreground">{{ u.reset }}</span>
                  </span>
                </span>
              </component>
            </li>
          </ul>
          <p v-if="currentStatus === 'running'" class="mt-1 px-2 text-2xs text-muted-foreground">
            {{ t('run.accountMenu.nextTurn') }}
          </p>
        </template>
      </div>
    </transition>
  </div>
  <!-- Bound to the resolved mode, so the menu ticks the mode the run really gets. -->
  <AppSelect
    :model-value="modeValue"
    size="xs"
    variant="ghost"
    class="shrink-0"
    :options="modeOptions"
    :trigger-label="modeChipLabel"
    :aria-label="modeChipTitle"
    :title="modeChipTitle"
    @update:model-value="permissionMode = $event"
  >
    <template #leading>
      <component
        :is="modeChipIcon"
        class="h-3 w-3 shrink-0"
        :class="modeValue === 'bypassPermissions' ? 'text-destructive' : modeValue === 'plan' ? 'text-warning' : 'text-muted-foreground'"
        :stroke-width="1.75"
      />
    </template>
  </AppSelect>
  <AppSelect
    v-if="effortLevelsNow.length"
    :model-value="effortValue"
    size="xs"
    variant="ghost"
    class="shrink-0"
    :options="effortOptions"
    :trigger-label="effortChipLabel"
    :aria-label="effortChipTitle"
    :title="effortChipTitle"
    @update:model-value="effortPick = $event"
  >
    <template #leading>
      <Gauge class="h-3 w-3 shrink-0 text-muted-foreground" :stroke-width="1.75" />
    </template>
  </AppSelect>
  <AppSelect
    :model-value="styleValue"
    size="xs"
    variant="ghost"
    class="shrink-0 max-w-[11rem]"
    :options="styleOptions"
    :trigger-label="styleChipLabel"
    :aria-label="styleChipTitle"
    :title="styleChipTitle"
    @update:model-value="stylePick = $event"
  >
    <template #leading>
      <MessageSquareText class="h-3 w-3 shrink-0 text-muted-foreground" :stroke-width="1.75" />
    </template>
  </AppSelect>
</template>
