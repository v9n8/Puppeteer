<script setup lang="ts">
// The composer toolbar's always-visible chips (UX-26): account (its menu lists
// the accounts and their plan usage, UX-47), permission mode, effort (only when
// the next turn's engine + model take one) and response style. Effort and style
// fold into Run options when `compact` (V3-34). State and defaults:
// useComposerChips; the menu's rows: useRunComposer.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { Check, ChevronDown, Gauge, Lock, MessageSquareText, Settings, UserCircle } from 'lucide-vue-next'
import AppSelect from '@/components/AppSelect.vue'
import { Button } from '@/components/ui'
import { useRunSession } from './useRunSession'

defineProps<{ compact?: boolean }>()

const { t } = useI18n()
const router = useRouter()
const {
  permissionMode, modeValue, modeOptions, modeChipLabel, modeChipIcon, modeChipTitle, effortPick, effortValue, effortLevelsNow, effortOptions,
  effortChipLabel, effortChipTitle, stylePick, styleValue, styleOptions, styleChipLabel, styleChipTitle,
  accountMenuOpen, accountMenuRoot, toggleAccountMenu, closeAccountMenu, accountUsage, accountExpired,
  accountModelLabel, composerModelLabel, accountChoices, selectedAccountId, accountSelectDisabled, handleAccountChange,
  effectiveEngine,
} = useRunSession()

const engineName = computed(() => (effectiveEngine.value === 'codex' ? 'Codex' : 'Claude'))
// The account's name alone: the model is in Run options.
const accountChipLabel = computed(() => {
  const suffix = composerModelLabel.value ? ` · ${composerModelLabel.value}` : ''
  return suffix && accountModelLabel.value.endsWith(suffix) ? accountModelLabel.value.slice(0, -suffix.length) : accountModelLabel.value
})
const accountChipTitle = computed(() => t('run.accountMenu.chipTitle', { account: accountChipLabel.value }))

function pickAccount(value: string) {
  closeAccountMenu()
  void handleAccountChange(value)
}
// The engine's card in Settings is the one place to add, sign in or remove accounts.
function manageAccounts() {
  closeAccountMenu()
  router.push({ path: '/settings', query: { section: 'engines' } }).catch(() => {})
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
      <span class="max-w-[10rem] truncate text-foreground">{{ accountChipLabel }}</span>
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
        class="absolute bottom-full left-0 z-30 mb-2 w-[380px] max-w-[calc(100vw-2rem)] rounded-md border border-border bg-popover p-2 shadow-lg shadow-black/20"
        :aria-label="t('run.accountMenu.heading', { engine: engineName })"
      >
        <h4 class="px-2 pb-1 pt-0.5 text-xs font-semibold">{{ t('run.accountMenu.heading', { engine: engineName }) }}</h4>
        <ul class="max-h-72 overflow-y-auto text-xs">
          <li v-for="a in accountChoices" :key="a.value ?? a.label">
            <component
              :is="a.value === null ? 'div' : 'button'"
              :type="a.value === null ? undefined : 'button'"
              class="flex w-full items-start gap-2 rounded px-2 py-1.5 text-left"
              :class="a.value === null
                ? ''
                : accountExpired(a.usageKey)
                  ? 'cursor-not-allowed opacity-60'
                  : 'cursor-pointer hover:bg-muted disabled:cursor-default disabled:opacity-50 disabled:hover:bg-transparent'"
              :disabled="a.value === null ? undefined : accountSelectDisabled"
              :aria-current="a.value === null || a.value === selectedAccountId ? 'true' : undefined"
              :aria-disabled="accountExpired(a.usageKey) ? 'true' : undefined"
              @click="a.value !== null && !accountExpired(a.usageKey) && pickAccount(a.value)"
            >
              <Lock v-if="accountExpired(a.usageKey)" class="mt-0.5 h-3 w-3 shrink-0 text-warning" :stroke-width="2" />
              <Check
                v-else
                class="mt-0.5 h-3 w-3 shrink-0 text-primary"
                :class="a.value === null || a.value === selectedAccountId ? 'opacity-100' : 'opacity-0'"
                :stroke-width="2.5"
              />
              <span class="flex min-w-0 flex-1 flex-col">
                <span class="truncate" :title="a.label">{{ a.label }}</span>
                <span
                  v-if="accountExpired(a.usageKey)"
                  class="text-2xs leading-tight text-warning"
                >{{ t('run.accountMenu.expired') }}</span>
                <span
                  v-else-if="a.detail"
                  class="truncate text-2xs leading-tight text-muted-foreground"
                  :title="a.detail"
                >{{ a.detail }}</span>
                <span
                  v-else-if="a.value === '' && effectiveEngine === 'claude'"
                  class="text-2xs leading-tight text-muted-foreground"
                >{{ t('run.accountMenu.followsProject') }}</span>
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
        <div class="mt-1 border-t border-border pt-1">
          <button
            type="button"
            class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-left text-xs text-foreground/80 hover:bg-muted"
            @click="manageAccounts"
          >
            <Settings class="h-3 w-3 shrink-0" :stroke-width="1.75" />
            {{ t('run.accountMenu.manage') }}
          </button>
        </div>
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
    v-if="!compact && effortLevelsNow.length"
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
    v-if="!compact"
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
