<script setup lang="ts">
// Settings › Model › "Save model changes?" (V3-63): what turns on and off, and
// each default the save moves off a model now off. The moves come from the
// `save_enabled_models` dry run, so the text matches what "Save" will write;
// nothing is worked out here.
import { useI18n } from 'vue-i18n'
import { Banner, Button, Modal } from '@/components/ui'
import { defaultChangePlace, type DefaultChange } from '@/lib/modelDefaultsUsage'

defineProps<{
  open: boolean
  summary: { on: string[]; off: string[] }
  changes: DefaultChange[]
  busy: boolean
}>()

const emit = defineEmits<{
  cancel: []
  confirm: []
  /** A link to the place a change lands in (it closes the dialog without saving). */
  openSection: [id: 'ai' | 'translate']
  openProject: [id: string]
}>()

const { t } = useI18n()

function placeLabel(c: DefaultChange): string {
  const { place } = defaultChangePlace(c)
  if (place === 'project') return t('settings.model.usage.project', { name: c.project_name ?? '' })
  const label = t(`settings.model.usage.${place}`)
  if (c.place === 'claude_model') return `${label} (Claude)`
  if (c.place === 'codex_model') return `${label} (Codex)`
  return label
}

// '' = the engine picks; a project then follows Settings (the backend writes NULL).
function toLabel(c: DefaultChange): string {
  if (c.to) return c.to
  return defaultChangePlace(c).place === 'project' ? t('settings.model.save.followSettings') : t('settings.ai.modelDefault')
}

function openPlace(c: DefaultChange) {
  const { section } = defaultChangePlace(c)
  if (section) emit('openSection', section)
  else if (c.project_id) emit('openProject', c.project_id)
}

const hasLink = (c: DefaultChange) => !!defaultChangePlace(c).section || !!c.project_id
</script>

<template>
  <Modal
    :open="open"
    :title="t('settings.model.save.title')"
    size="md"
    :closable="!busy"
    :dismiss-on-overlay="false"
    @close="emit('cancel')"
  >
    <div class="space-y-3 p-4" role="alertdialog" :aria-label="t('settings.model.save.title')">
      <p class="text-sm text-muted-foreground">
        <template v-if="summary.on.length">{{ t('settings.model.save.turnOn', { ids: summary.on.join(', ') }) }}</template>
        <template v-if="summary.on.length && summary.off.length"> · </template>
        <template v-if="summary.off.length">{{ t('settings.model.save.turnOff', { ids: summary.off.join(', ') }) }}</template>
      </p>
      <Banner v-for="(c, i) in changes" :key="i" tone="warning" class="text-sm">
        {{ t('settings.model.save.moved', { from: c.from, place: placeLabel(c), to: toLabel(c) }) }}
        <button
          v-if="hasLink(c)"
          type="button"
          class="cursor-pointer text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-sm"
          @click="openPlace(c)"
        >{{ t('settings.model.save.open', { place: placeLabel(c) }) }}</button>
      </Banner>
    </div>
    <template #footer>
      <Button variant="outline" :disabled="busy" @click="emit('cancel')">{{ t('settings.model.cancel') }}</Button>
      <Button :disabled="busy" @click="emit('confirm')">
        {{ busy ? t('settings.model.save.saving') : t('settings.model.save.confirm') }}
      </Button>
    </template>
  </Modal>
</template>
