<script setup lang="ts">
// Settings → "General": language, theme, terminal app, quit policy and launch at
// login. Theme and language apply on change, ahead of the debounced auto-save.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Palette } from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import { setLocale, SUPPORTED_LOCALES } from '@/i18n'
import { AppSelect, Card } from '@/components/ui'
import ColorThemePicker from '@/components/ColorThemePicker.vue'
import { useToast } from '@/composables/useToast'
import { applySystemAccent } from '@/lib/systemAccent'
import { TERMINAL_APPS, desktopOs } from '@/lib/terminalApps'
import { useSettingsForm } from './useSettingsForm'

const { t } = useI18n()
const { toast } = useToast()
const { settings, loading } = useSettingsForm()

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
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.language') }}</label>
        <AppSelect
          size="sm"
          v-model="settings.language"
          :options="SUPPORTED_LOCALES.map((l) => ({ value: l.value, label: l.label }))"
        />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.general.theme') }}</label>
        <AppSelect
          size="sm"
          v-model="settings.theme"
          :options="[
            { value: 'system', label: t('settings.general.themeSystem') },
            { value: 'light', label: t('settings.general.themeLight') },
            { value: 'dark', label: t('settings.general.themeDark') },
          ]"
        />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.general.colorTheme') }}</label>
        <p class="text-2xs text-muted-foreground">{{ t('settings.general.colorThemeHint') }}</p>
        <ColorThemePicker v-model="settings.color_theme" />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.general.animatedBackground') }}</label>
        <AppSelect
          size="sm"
          v-model="settings.animated_background"
          :options="[
            { value: 'true', label: t('settings.general.on') },
            { value: 'false', label: t('settings.general.off') },
          ]"
        />
        <p class="text-2xs text-muted-foreground">{{ t('settings.general.animatedBackgroundHint') }}</p>
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.general.terminalApp') }}</label>
        <AppSelect
          size="sm"
          v-model="settings.terminal_app"
          :options="terminalAppOptions"
        />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.general.quitPolicy') }}</label>
        <AppSelect
          size="sm"
          v-model="settings.quit_background_policy"
          :options="[
            { value: 'ask', label: t('settings.general.quitPolicyAsk') },
            { value: 'stop_all', label: t('settings.general.quitPolicyStopAll') },
            { value: 'keep_leftovers', label: t('settings.general.quitPolicyKeep') },
          ]"
        />
        <p class="text-2xs text-muted-foreground">{{ t('settings.general.quitPolicyHint') }}</p>
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.general.launchAtLogin') }}</label>
        <AppSelect
          size="sm"
          :model-value="launchAtLogin"
          :options="launchAtLoginOptions"
          @update:model-value="(v) => setLaunchAtLogin(v as LaunchAtLogin)"
        />
        <p class="text-2xs text-muted-foreground">{{ t('settings.general.launchAtLoginHint') }}</p>
      </div>
  </Card>
</template>
