<script setup lang="ts">
// Calendar header controls: ‹ Today › · Week / Month · Create event. Every
// button drives the store directly; the keys named in the tooltips are wired by
// useCalendarKeys in CalendarView.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronLeft, ChevronRight, Plus } from 'lucide-vue-next'
import { Button, SegmentedControl } from '@/components/ui'
import { useGoogleCalendarStore, type ViewMode } from '@/stores/googleCalendar'

defineProps<{ canCreate: boolean }>()
const emit = defineEmits<{ create: [] }>()

const { t } = useI18n()
const store = useGoogleCalendarStore()

const viewModeOptions = computed<{ value: ViewMode, label: string, title: string }[]>(() => [
  { value: 'week', label: t('calendar.view.week'), title: t('calendar.view.weekKey') },
  { value: 'month', label: t('calendar.view.month'), title: t('calendar.view.monthKey') },
])
</script>

<template>
  <div class="flex items-center gap-1">
    <Button
      variant="outline"
      size="icon"
      :aria-label="t('calendar.view.previousKey')"
      :title="t('calendar.view.previousKey')"
      @click="store.step(-1)"
    >
      <ChevronLeft class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
    <Button variant="outline" size="sm" :title="t('calendar.view.todayKey')" @click="store.today()">
      {{ t('calendar.view.today') }}
    </Button>
    <Button
      variant="outline"
      size="icon"
      :aria-label="t('calendar.view.nextKey')"
      :title="t('calendar.view.nextKey')"
      @click="store.step(1)"
    >
      <ChevronRight class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
  </div>

  <SegmentedControl
    :model-value="store.viewMode"
    :options="viewModeOptions"
    :stretch="false"
    :aria-label="t('calendar.view.viewMode')"
    @update:model-value="store.setViewMode"
  />

  <Button
    v-if="canCreate"
    variant="primary"
    size="sm"
    :aria-label="t('calendar.createEvent')"
    :title="t('calendar.view.createKey')"
    @click="emit('create')"
  >
    <Plus class="h-3.5 w-3.5" :stroke-width="1.75" />
    <span class="max-[1000px]:hidden">{{ t('calendar.createEvent') }}</span>
  </Button>
</template>
