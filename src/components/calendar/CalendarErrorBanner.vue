<script setup lang="ts">
// Warnings above the grid: accounts without calendar write scope, and one row
// per account whose fetch failed. "Retry" re-fetches only that account;
// "Reconnect" is offered only when the error is a permission one.
import { useI18n } from 'vue-i18n'
import { Loader2 } from 'lucide-vue-next'
import { Banner, Button } from '@/components/ui'
import { isAuthError, useGoogleCalendarStore } from '@/stores/googleCalendar'

defineEmits<{ reconnect: [] }>()

const { t } = useI18n()
const store = useGoogleCalendarStore()
</script>

<template>
  <div
    v-if="store.accountsMissingWrite.length || store.errors.length"
    class="flex flex-col gap-1.5 px-6 py-2 border-b border-border/60 shrink-0"
  >
    <Banner v-if="store.accountsMissingWrite.length" tone="warning">
      {{ t('calendar.missingWriteWarning') }}
      <b>{{ store.accountsMissingWrite.map(a => a.label).join(', ') }}</b>.
      {{ t('calendar.missingWriteHint') }}
      <template #actions>
        <Button variant="outline" size="xs" @click="$emit('reconnect')">{{ t('calendar.reconnect') }}</Button>
      </template>
    </Banner>
    <Banner v-for="err in store.errors" :key="err.account_id" tone="warning">
      {{ err.account_label }}: {{ err.message }}
      <template #actions>
        <Button
          variant="outline"
          size="xs"
          :disabled="store.retrying.has(err.account_id)"
          @click="store.retryAccount(err.account_id)"
        >
          <Loader2 v-if="store.retrying.has(err.account_id)" class="h-3 w-3 animate-spin" :stroke-width="2" />
          {{ t('calendar.view.retry') }}
        </Button>
        <Button v-if="isAuthError(err.message)" variant="outline" size="xs" @click="$emit('reconnect')">
          {{ t('calendar.reconnect') }}
        </Button>
      </template>
    </Banner>
  </div>
</template>
