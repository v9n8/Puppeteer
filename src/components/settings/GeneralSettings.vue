<script setup lang="ts">
// Settings → "General", in three cards: Appearance (light / dark mode, color
// theme, row density), Text (preview, font, weight, smoothing, text sizes, code
// font) and App (language, terminal app, launch at login, quit policy). Theme
// and language apply on change, ahead of the debounced auto-save; the Text
// card and the row density are per-machine (localStorage) and apply at once.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { AppWindow, Monitor, Moon, Palette, RotateCcw, Rows2, Rows3, Rows4, Sun, Type } from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import { setLocale, SUPPORTED_LOCALES } from '@/i18n'
import { AppSelect, Button, Card, Input, SegmentedControl, type SegmentedOption } from '@/components/ui'
import ColorThemePicker from '@/components/ColorThemePicker.vue'
import { useToast } from '@/composables/useToast'
import { normalizeColorTheme } from '@/lib/colorThemes'
import { TERMINAL_APPS, desktopOs } from '@/lib/terminalApps'
import { useUILayoutStore, type ListDensity } from '@/stores/uiLayout'
import {
  APP_FONT_SIZES, CHAT_FONT_SIZES, DEFAULT_FONT_PREFS, FONT_WEIGHTS, resolveSmoothing, supportsWeight,
  type FontFamilyId, type FontSmoothing, type FontWeight, type MonoFontId,
} from '@/lib/appFonts'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'
import FontFamilyPicker from './FontFamilyPicker.vue'
import SizeStepper from './SizeStepper.vue'
import TypographyPreview from './TypographyPreview.vue'

const { t } = useI18n()
const { toast } = useToast()
const { settings, loading } = useSettingsForm()

const modeOptions = computed<SegmentedOption<string>[]>(() => [
  { value: 'system', label: t('settings.general.themeSystem'), icon: Monitor },
  { value: 'light', label: t('settings.general.themeLight'), icon: Sun },
  { value: 'dark', label: t('settings.general.themeDark'), icon: Moon },
])

// Row density is a per-machine display preference kept in the uiLayout store
// (localStorage), not in the settings table, so it applies on change and the
// session list and worker panel follow at once.
const uiLayout = useUILayoutStore()
const densityOptions = computed<SegmentedOption<ListDensity>[]>(() => [
  { value: 'comfortable', label: t('settings.general.rowDensityComfortable'), icon: Rows2 },
  { value: 'compact', label: t('settings.general.rowDensityCompact'), icon: Rows3 },
  { value: 'dense', label: t('settings.general.rowDensityDense'), icon: Rows4 },
])

// Text: per-machine too (lib/appFonts), applied on change.
const fonts = computed(() => uiLayout.fontPrefs)

const WEIGHT_LABEL_KEYS: Record<FontWeight, string> = { 400: 'weightRegular', 450: 'weightMedium', 500: 'weightBold' }
const weightOptions = computed<SegmentedOption<FontWeight>[]>(() =>
  FONT_WEIGHTS.filter((w) => supportsWeight(fonts.value.family, w))
    .map((w) => ({ value: w, label: t(`settings.general.${WEIGHT_LABEL_KEYS[w]}`) })),
)
const smoothingOptions = computed<SegmentedOption<FontSmoothing>[]>(() => [
  { value: 'auto', label: t('settings.general.smoothingAuto') },
  { value: 'crisp', label: t('settings.general.smoothingCrisp') },
  { value: 'thin', label: t('settings.general.smoothingThin') },
])
const monoOptions = computed<SegmentedOption<MonoFontId>[]>(() => [
  { value: 'jetbrains', label: t('settings.general.monoJetbrains') },
  { value: 'plexmono', label: t('settings.general.monoPlexmono') },
  { value: 'system', label: t('settings.general.monoSystem') },
])

// What "Automatic" resolves to on the screen this window is on.
const smoothingNow = computed(() =>
  resolveSmoothing('auto', window.devicePixelRatio) === 'crisp'
    ? t('settings.general.smoothingNowCrisp')
    : t('settings.general.smoothingNowThin'),
)

function setFamily(family: FontFamilyId) {
  // The in-between weight doesn't exist in a font with fixed weights.
  const weight = supportsWeight(family, fonts.value.weight) ? fonts.value.weight : 400
  uiLayout.setFontPrefs({ family, weight })
}

// The custom font name survives a reset, so picking "Custom" again restores it.
const RESET_KEYS = ['family', 'weight', 'smoothing', 'mono', 'appSize', 'chatSize'] as const
const textIsDefault = computed(() => RESET_KEYS.every((k) => fonts.value[k] === DEFAULT_FONT_PREFS[k]))
function resetText() {
  uiLayout.setFontPrefs({ ...DEFAULT_FONT_PREFS, customFont: fonts.value.customFont })
}

const previewSummary = computed(() => {
  const f = fonts.value
  const family = f.family === 'custom' && f.customFont.trim()
    ? f.customFont.trim()
    : t(`settings.general.font${f.family.charAt(0).toUpperCase()}${f.family.slice(1)}`)
  return [
    family,
    `${t(`settings.general.${WEIGHT_LABEL_KEYS[f.weight]}`)} ${f.weight}`,
    smoothingOptions.value.find((o) => o.value === f.smoothing)?.label,
    `${f.appSize} / ${f.chatSize} px`,
  ].join(' · ')
})

// Only this OS's terminals: the backend reads any other value as this OS's default.
const terminalApps = TERMINAL_APPS[desktopOs(navigator.userAgent)]
const terminalAppOptions = computed(() =>
  terminalApps.map((a) => ({ value: a.value, label: t(`settings.general.${a.labelKey}`) })),
)

// Launch at login lives in the OS's login item, not in the settings table, so
// it is applied on change instead of going through the debounced auto-save.
type LaunchAtLogin = 'off' | 'on' | 'minimized'
const launchAtLogin = ref<LaunchAtLogin>('off')
const launchAtLoginOptions = computed(() => [
  { value: 'off', label: t('settings.general.launchAtLoginOff') },
  { value: 'on', label: t('settings.general.launchAtLoginOn') },
  { value: 'minimized', label: t('settings.general.launchAtLoginMinimized') },
])
type AutostartStatus = { enabled: boolean; minimized: boolean }
const autostartMode = (s: AutostartStatus): LaunchAtLogin =>
  !s.enabled ? 'off' : s.minimized ? 'minimized' : 'on'

async function loadLaunchAtLogin() {
  try {
    launchAtLogin.value = autostartMode(await invoke<AutostartStatus>('autostart_get'))
  } catch {
    // Unsupported platform — leave the select at "off".
  }
}

async function setLaunchAtLogin(mode: LaunchAtLogin) {
  const previous = launchAtLogin.value
  launchAtLogin.value = mode
  try {
    const status = await invoke<AutostartStatus>('autostart_set', {
      enabled: mode !== 'off',
      minimized: mode === 'minimized',
    })
    launchAtLogin.value = autostartMode(status)
    toast.success(t('settings.toast.saved'))
  } catch (e) {
    launchAtLogin.value = previous
    toast.error(String(e))
  }
}

onMounted(() => {
  loadLaunchAtLogin()
})

function applyTheme(theme: string) {
  if (theme === 'dark') {
    document.documentElement.classList.add('dark')
  } else if (theme === 'light') {
    document.documentElement.classList.remove('dark')
  } else {
    const dark = window.matchMedia('(prefers-color-scheme: dark)').matches
    document.documentElement.classList.toggle('dark', dark)
  }
}

function applyColorTheme(theme: string) {
  const t = normalizeColorTheme(theme)
  if (t !== 'default') document.documentElement.setAttribute('data-theme', t)
  else document.documentElement.removeAttribute('data-theme')
}

// Apply theme instantly (don't wait for the debounced save).
watch(() => settings.value.theme, (v) => {
  if (!loading.value) applyTheme(v)
})
watch(() => settings.value.color_theme, (v) => {
  if (!loading.value) applyColorTheme(v)
})
// Apply language instantly (don't wait for the debounced save).
watch(() => settings.value.language, (v) => {
  if (!loading.value) setLocale(v)
})
</script>

<template>
  <div class="space-y-4">
    <div>
      <h3 class="text-lg font-semibold">{{ t('settings.sections.general') }}</h3>
      <p class="mt-0.5 text-xs text-muted-foreground">{{ t('settings.general.intro') }}</p>
    </div>

    <Card body-class="divide-y divide-border">
      <template #header>
        <Palette class="h-4 w-4 text-muted-foreground" :stroke-width="1.75" />
        <span class="text-sm font-semibold">{{ t('settings.general.sectionAppearance') }}</span>
      </template>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.mode')" :description="t('settings.general.modeDesc')">
        <div class="flex justify-end">
          <SegmentedControl v-model="settings.theme" :options="modeOptions" :stretch="false" />
        </div>
      </SettingsRow>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.colorTheme')" :description="t('settings.general.colorThemeHint')" stacked>
        <ColorThemePicker v-model="settings.color_theme" class="mt-1.5" />
      </SettingsRow>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.rowDensity')" :description="t('settings.general.rowDensityHint')">
        <div class="flex justify-end">
          <SegmentedControl
            :model-value="uiLayout.listDensity"
            :options="densityOptions"
            :stretch="false"
            @update:model-value="uiLayout.setListDensity"
          />
        </div>
      </SettingsRow>
    </Card>

    <Card body-class="divide-y divide-border">
      <template #header>
        <Type class="h-4 w-4 text-muted-foreground" :stroke-width="1.75" />
        <span class="text-sm font-semibold">{{ t('settings.general.sectionText') }}</span>
        <span class="ml-1 inline-flex items-center gap-1 text-xs text-muted-foreground">
          <Monitor class="h-4 w-4" :stroke-width="1.75" />{{ t('settings.general.deviceOnly') }}
        </span>
        <Button variant="ghost" size="sm" class="ml-auto -my-1" :disabled="textIsDefault" @click="resetText">
          <RotateCcw class="h-4 w-4" :stroke-width="1.75" />{{ t('settings.general.resetText') }}
        </Button>
      </template>
      <div class="p-4">
        <TypographyPreview :summary="previewSummary" />
      </div>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.font')" :description="t('settings.general.fontDesc')" stacked>
        <FontFamilyPicker :model-value="fonts.family" class="mt-1.5" @update:model-value="setFamily" />
        <div v-if="fonts.family === 'custom'" class="mt-3 flex flex-wrap items-center gap-2">
          <Input
            size="sm"
            class="w-72"
            :model-value="fonts.customFont"
            :placeholder="t('settings.general.fontCustomPlaceholder')"
            @change="uiLayout.setFontPrefs({ customFont: ($event.target as HTMLInputElement).value })"
          />
          <span class="text-xs text-muted-foreground">{{ t('settings.general.fontCustomFallback') }}</span>
        </div>
      </SettingsRow>
      <SettingsRow
        class="px-4 py-3"
        :label="t('settings.general.weight')"
        :description="supportsWeight(fonts.family, 450) ? t('settings.general.weightDesc') : t('settings.general.weightStaticDesc')"
      >
        <div class="flex justify-end">
          <SegmentedControl
            :model-value="fonts.weight"
            :options="weightOptions"
            :stretch="false"
            @update:model-value="(weight) => uiLayout.setFontPrefs({ weight })"
          />
        </div>
      </SettingsRow>
      <SettingsRow
        class="px-4 py-3"
        :label="t('settings.general.smoothing')"
        :description="fonts.smoothing === 'auto'
          ? `${t('settings.general.smoothingDesc')} ${smoothingNow}`
          : t('settings.general.smoothingDesc')"
      >
        <div class="flex justify-end">
          <SegmentedControl
            :model-value="fonts.smoothing"
            :options="smoothingOptions"
            :stretch="false"
            @update:model-value="(smoothing) => uiLayout.setFontPrefs({ smoothing })"
          />
        </div>
      </SettingsRow>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.appFontSize')" :description="t('settings.general.appFontSizeDesc')">
        <div class="flex justify-end">
          <SizeStepper
            :model-value="fonts.appSize"
            :sizes="APP_FONT_SIZES"
            @update:model-value="(appSize) => uiLayout.setFontPrefs({ appSize })"
          />
        </div>
      </SettingsRow>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.chatFontSize')" :description="t('settings.general.chatFontSizeDesc')">
        <div class="flex justify-end">
          <SizeStepper
            :model-value="fonts.chatSize"
            :sizes="CHAT_FONT_SIZES"
            @update:model-value="(chatSize) => uiLayout.setFontPrefs({ chatSize })"
          />
        </div>
      </SettingsRow>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.monoFont')" :description="t('settings.general.monoFontDesc')">
        <div class="flex justify-end">
          <SegmentedControl
            :model-value="fonts.mono"
            :options="monoOptions"
            :stretch="false"
            @update:model-value="(mono) => uiLayout.setFontPrefs({ mono })"
          />
        </div>
      </SettingsRow>
    </Card>

    <Card body-class="divide-y divide-border">
      <template #header>
        <AppWindow class="h-4 w-4 text-muted-foreground" :stroke-width="1.75" />
        <span class="text-sm font-semibold">{{ t('settings.general.sectionApp') }}</span>
      </template>
      <SettingsRow class="px-4 py-3" :label="t('settings.language')">
        <AppSelect
          size="sm"
          v-model="settings.language"
          :options="SUPPORTED_LOCALES.map((l) => ({ value: l.value, label: l.label }))"
        />
      </SettingsRow>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.terminalApp')" :description="t('settings.general.terminalAppDesc')">
        <AppSelect
          size="sm"
          v-model="settings.terminal_app"
          :options="terminalAppOptions"
        />
      </SettingsRow>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.launchAtLogin')" :description="t('settings.general.launchAtLoginHint')">
        <AppSelect
          size="sm"
          :model-value="launchAtLogin"
          :options="launchAtLoginOptions"
          @update:model-value="(v) => setLaunchAtLogin(v as LaunchAtLogin)"
        />
      </SettingsRow>
      <SettingsRow class="px-4 py-3" :label="t('settings.general.quitPolicy')" :description="t('settings.general.quitPolicyHint')">
        <AppSelect
          size="sm"
          v-model="settings.quit_background_policy"
          :options="[
            { value: 'ask', label: t('settings.general.quitPolicyAsk') },
            { value: 'stop_all', label: t('settings.general.quitPolicyStopAll') },
            { value: 'keep_leftovers', label: t('settings.general.quitPolicyKeep') },
          ]"
        />
      </SettingsRow>
    </Card>
  </div>
</template>
