<script setup lang="ts">
// Settings → AI › Sessions › Notifications: the desktop OS notification
// switches, one per kind plus the sound, all kept in the `desktop_notify` JSON
// setting (lib/desktopNotify.ts). Local-only with Remote Control.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ToggleSwitch } from '@/components/ui'
import { DESKTOP_NOTIFY_KINDS, parseDesktopNotify, type DesktopNotifyKind } from '@/lib/desktopNotify'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'

const { t } = useI18n()
const { settings } = useSettingsForm()

const notify = computed(() => parseDesktopNotify(settings.value.desktop_notify))

// Writes every field, so the stored value says what the screen shows.
function toggle(kind: DesktopNotifyKind) {
  settings.value.desktop_notify = JSON.stringify({ ...notify.value, [kind]: !notify.value[kind] })
}
</script>

<template>
  <SettingsRow
    v-for="k in DESKTOP_NOTIFY_KINDS"
    :key="k"
    :label="t(`settings.sessions.notify.${k}`)"
    :description="t(`settings.sessions.notifyDescriptions.${k}`)"
  >
    <div class="flex min-[900px]:justify-end">
      <ToggleSwitch
        size="md"
        :checked="notify[k]"
        :label="t(`settings.sessions.notify.${k}`)"
        @toggle="toggle(k)"
      />
    </div>
  </SettingsRow>
</template>
