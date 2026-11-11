<script setup lang="ts">
// Settings → "General": language, theme, row density, terminal app, quit policy
// and launch at login. Theme and language apply on change, ahead of the
// debounced auto-save.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Palette, Rows2, Rows3, Rows4 } from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import { setLocale, SUPPORTED_LOCALES } from '@/i18n'
import { AppSelect, Card, SegmentedControl, type SegmentedOption } from '@/components/ui'
import ColorThemePicker from '@/components/ColorThemePicker.vue'
import { useToast } from '@/composables/useToast'
import { applySystemAccent } from '@/lib/systemAccent'
import { TERMINAL_APPS, desktopOs } from '@/lib/terminalApps'
import { useUILayoutStore, type ListDensity } from '@/stores/uiLayout'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'

const { t } = useI18n()
const { toast } = useToast()
const { settings, loading } = useSettingsForm()

// Row density is a per-machine display preference kept in the uiLayout store
// (localStorage), not in the settings table, so it applies on change and the
// session list and worker panel follow at once.
const uiLayout = useUILayoutStore()
const densityOptions = computed<SegmentedOption<ListDensity>[]>(() => [
  { value: 'comfortable', label: t('settings.general.rowDensityComfortable'), icon: Rows2 },
  { value: 'compact', label: t('settings.general.rowDensityCompact'), icon: Rows3 },
  { value: 'dense', label: t('settings.general.rowDensityDense'), icon: Rows4 },
])

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
  const t = theme && theme !== 'default' ? theme : ''
  if (t) document.documentElement.setAttribute('data-theme', t)
  else document.documentElement.removeAttribute('data-theme')
  applySystemAccent(t)
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
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Palette class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.sections.general') }}</span>
    </template>
      <SettingsRow :label="t('settings.language')">
        <AppSelect
          size="sm"
          v-model="settings.language"
          :options="SUPPORTED_LOCALES.map((l) => ({ value: l.value, label: l.label }))"
        />
      </SettingsRow>
      <SettingsRow :label="t('settings.general.theme')">
        <AppSelect
          size="sm"
          v-model="settings.theme"
          :options="[
            { value: 'system', label: t('settings.general.themeSystem') },
            { value: 'light', label: t('settings.general.themeLight') },
            { value: 'dark', label: t('settings.general.themeDark') },
          ]"
        />
      </SettingsRow>
      <SettingsRow :label="t('settings.general.colorTheme')" :hint="t('settings.general.colorThemeHint')" stacked>
        <ColorThemePicker v-model="settings.color_theme" />
      </SettingsRow>
      <SettingsRow :label="t('settings.general.animatedBackground')" :hint="t('settings.general.animatedBackgroundHint')">
        <AppSelect
          size="sm"
          v-model="settings.animated_background"
          :options="[
            { value: 'true', label: t('settings.general.on') },
            { value: 'false', label: t('settings.general.off') },
          ]"
        />
      </SettingsRow>
      <SettingsRow :label="t('settings.general.rowDensity')" :hint="t('settings.general.rowDensityHint')">
        <SegmentedControl
          :model-value="uiLayout.listDensity"
          :options="densityOptions"
          @update:model-value="uiLayout.setListDensity"
        />
      </SettingsRow>
      <SettingsRow :label="t('settings.general.terminalApp')">
        <AppSelect
          size="sm"
          v-model="settings.terminal_app"
          :options="terminalAppOptions"
        />
      </SettingsRow>
      <SettingsRow :label="t('settings.general.quitPolicy')" :hint="t('settings.general.quitPolicyHint')">
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
      <SettingsRow :label="t('settings.general.launchAtLogin')" :hint="t('settings.general.launchAtLoginHint')">
        <AppSelect
          size="sm"
          :model-value="launchAtLogin"
          :options="launchAtLoginOptions"
          @update:model-value="(v) => setLaunchAtLogin(v as LaunchAtLogin)"
        />
      </SettingsRow>
  </Card>
</template>
