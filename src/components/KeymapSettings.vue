<script setup lang="ts">
// Settings → Keyboard shortcuts: the native menu's actions and their keys
// (lib/keymap). A change goes straight onto the menu, which shows and fires it.
// While a new shortcut is being recorded the menu has none, so the keys reach
// this page — and a taken one can be reported — instead of running an action.
import { computed, onBeforeUnmount, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Keyboard, RotateCcw, X } from 'lucide-vue-next'
import { Button, Card } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { IS_MAC, effectiveKeymap, menuActions, suspendAccelerators } from '@/lib/appMenu'
import { errorText } from '@/lib/featureGate'
import {
  KEYMAP_SETTING_KEY,
  acceleratorFromEvent,
  acceleratorProblem,
  formatAccelerator,
  keymapConflict,
  parseKeymap,
  resetBinding,
  serializeKeymap,
  setBinding,
  type Keymap,
} from '@/lib/keymap'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const appSettings = useAppSettingsStore()
const { toast } = useToast()

const actions = computed(() => menuActions())
const changes = computed(() => parseKeymap(appSettings.settings?.keymap_config))
const resolved = computed(() => effectiveKeymap())
const labelOf = computed(() => new Map(actions.value.map((a) => [a.id, a.label])))

const keys = (accel: string) => formatAccelerator(accel, IS_MAC)
const systemKeys = { quit: keys('CmdOrCtrl+Q'), close: keys('CmdOrCtrl+W'), minimize: keys('CmdOrCtrl+M') }
const isChanged = (id: string) => Object.prototype.hasOwnProperty.call(changes.value, id)

const recording = ref<string | null>(null)
const problem = ref<{ id: string; text: string } | null>(null)

async function save(next: Keymap) {
  const value = serializeKeymap(next)
  // Shown at once (the menu follows this row); reloaded if the write fails.
  if (appSettings.settings) appSettings.settings.keymap_config = value
  try {
    await invoke('update_setting', { key: KEYMAP_SETTING_KEY, value })
  } catch (e) {
    await appSettings.refresh().catch(() => {})
    toast.error(errorText(e, t))
  }
}

function startRecording(id: string) {
  problem.value = null
  if (!recording.value) {
    suspendAccelerators(true)
    window.addEventListener('keydown', onRecordKey, true)
    // Leaving the app gives the menu its shortcuts back.
    window.addEventListener('blur', stopRecording)
  }
  recording.value = id
}

function stopRecording() {
  if (!recording.value) return
  recording.value = null
  suspendAccelerators(false)
  window.removeEventListener('keydown', onRecordKey, true)
  window.removeEventListener('blur', stopRecording)
}

const MODIFIER_KEYS = new Set(['Meta', 'Control', 'Alt', 'Shift', 'CapsLock', 'Fn', 'OS'])

// Capture phase on window: the press never reaches the page's own shortcuts
// (⌘F, the palette, Esc closing a dialog).
function onRecordKey(e: KeyboardEvent) {
  const id = recording.value
  if (!id || e.isComposing) return
  e.preventDefault()
  e.stopImmediatePropagation()
  if (MODIFIER_KEYS.has(e.key)) return
  const bare = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey
  stopRecording()
  if (bare && e.key === 'Escape') return

  const accel = acceleratorFromEvent(e, IS_MAC)
  const why = accel ? acceleratorProblem(accel, IS_MAC) : 'invalid'
  if (!accel || why) {
    problem.value = {
      id,
      text: t(`keymap.problem.${why}`, { keys: accel ? keys(accel) : '', mod: IS_MAC ? '⌘' : 'Ctrl' }),
    }
    return
  }
  const other = keymapConflict(resolved.value, id, accel, IS_MAC)
  if (other) {
    problem.value = { id, text: t('keymap.problem.taken', { keys: keys(accel), action: labelOf.value.get(other) ?? other }) }
    return
  }
  void save(setBinding(changes.value, id, accel))
}

function remove(id: string) {
  problem.value = null
  void save(setBinding(changes.value, id, ''))
}

function reset(id: string) {
  problem.value = null
  void save(resetBinding(changes.value, id))
}

function resetAll() {
  problem.value = null
  void save({})
}

onBeforeUnmount(stopRecording)
</script>

<template>
  <Card body-class="p-4 space-y-3">
    <template #header>
      <Keyboard class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="flex-1 text-xs font-semibold">{{ t('keymap.title') }}</span>
      <Button variant="outline" size="xs" :disabled="!Object.keys(changes).length" @click="resetAll">
        <RotateCcw class="h-3 w-3" :stroke-width="1.75" />
        {{ t('keymap.resetAll') }}
      </Button>
    </template>

    <p class="text-2xs text-muted-foreground">{{ t('keymap.help', systemKeys) }}</p>

    <div class="divide-y divide-border/60 rounded-lg border border-border/60">
      <div v-for="action in actions" :key="action.id" class="px-3 py-1.5">
        <div class="flex items-center gap-2">
          <span class="min-w-0 flex-1 truncate text-sm text-foreground">{{ action.label }}</span>
          <template v-if="recording === action.id">
            <span class="text-2xs text-primary">{{ t('keymap.recording') }}</span>
            <Button variant="ghost" size="xs" @click="stopRecording">{{ t('keymap.cancel') }}</Button>
          </template>
          <template v-else>
            <span
              v-if="resolved.get(action.id)"
              class="rounded border border-border bg-muted/40 px-1.5 py-0.5 font-mono text-2xs text-foreground"
            >{{ keys(resolved.get(action.id)!) }}</span>
            <span v-else class="text-2xs text-muted-foreground">{{ t('keymap.none') }}</span>
            <Button variant="ghost" size="xs" @click="startRecording(action.id)">{{ t('keymap.change') }}</Button>
            <Button
              variant="ghost"
              size="icon-xs"
              :disabled="!resolved.get(action.id)"
              :title="t('keymap.remove')"
              :aria-label="t('keymap.remove')"
              @click="remove(action.id)"
            >
              <X class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
            <Button
              variant="ghost"
              size="icon-xs"
              :disabled="!isChanged(action.id)"
              :title="t('keymap.reset')"
              :aria-label="t('keymap.reset')"
              @click="reset(action.id)"
            >
              <RotateCcw class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
          </template>
        </div>
        <p v-if="problem?.id === action.id" class="mt-1 text-2xs text-destructive">{{ problem.text }}</p>
      </div>
    </div>
  </Card>
</template>
