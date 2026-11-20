<script setup lang="ts">
// The composer toolbar's always-visible chips (UX-26): account (its menu lists
// the account names only, UR-55: plan usage is the status bar's), permission
// mode, effort (only when the next turn's engine + model take one) and response
// style. Effort and style fold into Run options when `compact` (V3-34). State
// and defaults: useComposerChips; the menu's accounts: useRunComposer, its
// ✓ / ★: lib/accountMenu.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, ChevronDown, Gauge, Lock, MessageSquareText, Star, UserCircle } from 'lucide-vue-next'
import AppSelect from '@/components/AppSelect.vue'
import { Button, DropdownLabel } from '@/components/ui'
import ResponseStyleModal from '@/components/settings/ResponseStyleModal.vue'
import { accountMenuRows } from '@/lib/accountMenu'
import { useRunSession } from './useRunSession'

const props = defineProps<{ compact?: boolean }>()

const { t } = useI18n()
const {
  permissionMode, modeValue, modeOptions, modeChipLabel, modeChipIcon, modeChipTitle, effortPick, effortValue, effortLevelsNow, effortOptions,
  effortChipLabel, effortChipTitle, styleValue, styleOptions, styleChipLabel, styleChipTitle,
  accountMenuOpen, accountMenuRoot, toggleAccountMenu, closeAccountMenu, accountExpired,
  accountModelLabel, composerModelLabel, accountChoices, selectedAccountId, accountSelectDisabled, handleAccountChange,
  effectiveProjectClaudeAccountId, effectiveEngine, projectId, styleDialogOpen, pickStyle, onStyleSaved,
} = useRunSession()

const engineName = computed(() => (effectiveEngine.value === 'codex' ? 'Codex' : 'Claude'))
// The account's name alone: the model is in Run options.
const accountChipLabel = computed(() => {
  const suffix = composerModelLabel.value ? ` · ${composerModelLabel.value}` : ''
  return suffix && accountModelLabel.value.endsWith(suffix) ? accountModelLabel.value.slice(0, -suffix.length) : accountModelLabel.value
})
const accountChipTitle = computed(() => t('run.accountMenu.chipTitle', { account: accountChipLabel.value }))

// ★ = the account a run with no pick runs on: the Project's (else the global
// default) Claude account, the shared login for Codex.
const accountRows = computed(() =>
  accountMenuRows(
    accountChoices.value,
    effectiveEngine.value === 'codex' ? '' : effectiveProjectClaudeAccountId.value,
    selectedAccountId.value,
    accountExpired,
  ),
)

function pickAccount(value: string) {
  closeAccountMenu()
  void handleAccountChange(value)
}
</script>

<template>
  <div ref="accountMenuRoot" class="relative inline-flex min-w-0" @keydown.esc.stop="closeAccountMenu">
    <Button
      variant="outline"
      size="xs"
      class="min-w-0 rounded-sm font-normal"
      :class="props.compact ? 'gap-0.5 px-1.5' : 'gap-1.5 px-2'"
      :title="accountChipTitle"
      :aria-label="accountChipTitle"
      aria-haspopup="dialog"
      :aria-expanded="accountMenuOpen"
      @click="toggleAccountMenu"
    >
      <UserCircle class="h-3 w-3 shrink-0" :stroke-width="1.75" />
      <span v-if="!props.compact" class="max-w-[10rem] truncate text-foreground">{{ accountChipLabel }}</span>
      <ChevronDown
        v-if="!props.compact"
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
        class="absolute bottom-full left-0 z-30 mb-2 w-[240px] max-w-[calc(100vw-2rem)] rounded-md border border-border bg-popover p-1 text-xs shadow-lg shadow-black/20"
        :aria-label="t('run.accountMenu.heading', { engine: engineName })"
      >
        <DropdownLabel class="px-2 pb-1 pt-1">{{ t('run.accountMenu.heading', { engine: engineName }) }}</DropdownLabel>
        <ul class="max-h-72 overflow-y-auto">
          <li v-for="a in accountRows" :key="a.value ?? a.label">
            <component
              :is="a.value === null ? 'div' : 'button'"
              :type="a.value === null ? undefined : 'button'"
              class="flex h-8 w-full items-center gap-2 rounded px-2 text-left"
              :class="a.value === null
                ? ''
                : a.expired
                  ? 'cursor-not-allowed opacity-60'
                  : 'cursor-pointer hover:bg-muted disabled:cursor-default disabled:opacity-50 disabled:hover:bg-transparent'"
              :title="a.expired ? t('run.accountMenu.expired') : a.label"
              :disabled="a.value === null ? undefined : accountSelectDisabled"
              :aria-current="a.isCurrent ? 'true' : undefined"
              :aria-disabled="a.expired ? 'true' : undefined"
              @click="a.pick !== null ? pickAccount(a.pick) : !a.expired && closeAccountMenu()"
            >
              <Lock v-if="a.expired" class="h-3 w-3 shrink-0 text-warning" :stroke-width="2" />
              <Check
                v-else
                class="h-3 w-3 shrink-0 text-primary"
                :class="a.isCurrent ? 'opacity-100' : 'opacity-0'"
                :stroke-width="2.5"
              />
              <!-- ★ before the label, as AppSelect marks its default option (UR-22). -->
              <span class="flex min-w-0 items-center gap-1">
                <span
                  v-if="a.isDefault"
                  class="inline-flex shrink-0"
                  role="img"
                  :title="t('common.default')"
                  :aria-label="t('common.default')"
                >
                  <Star class="h-3 w-3 fill-primary text-primary" :stroke-width="2" />
                </span>
                <span class="min-w-0 truncate">{{ a.label }}</span>
              </span>
              <span class="flex-1" />
              <span v-if="a.expired" class="shrink-0 text-2xs text-warning">{{ t('run.accountMenu.expiredShort') }}</span>
            </component>
          </li>
        </ul>
      </div>
    </transition>
  </div>
  <!-- Bound to the resolved mode, so the menu ticks the mode the run really gets. -->
  <AppSelect
    :model-value="modeValue"
    size="xs"
    variant="outline"
    class="shrink-0 [&>button]:gap-1.5 [&>button]:rounded-sm [&>button]:pl-2"
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
    v-if="!compact && effortLevelsNow.length"
    :model-value="effortValue"
    size="xs"
    variant="outline"
    class="shrink-0 [&>button]:gap-1.5 [&>button]:rounded-sm [&>button]:pl-2"
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
    v-if="!compact"
    :model-value="styleValue"
    size="xs"
    variant="outline"
    class="shrink-0 [&>button]:gap-1.5 [&>button]:rounded-sm [&>button]:pl-2 max-w-[11rem]"
    :options="styleOptions"
    :trigger-label="styleChipLabel"
    :aria-label="styleChipTitle"
    :title="styleChipTitle"
    @update:model-value="pickStyle"
  >
    <template #leading>
      <MessageSquareText class="h-3 w-3 shrink-0 text-muted-foreground" :stroke-width="1.75" />
    </template>
  </AppSelect>
  <!-- Also opened from the Run options select when compact, so it lives here once. -->
  <ResponseStyleModal v-model:open="styleDialogOpen" :target="null" :project-id="projectId" @saved="onStyleSaved" />
</template>
