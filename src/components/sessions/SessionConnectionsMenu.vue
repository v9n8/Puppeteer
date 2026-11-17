<script setup lang="ts">
// "Connections" button of the session info bar (V3-16): a plug icon with the
// number of git accounts, servers and cloud accounts the session can use, and a
// menu listing them by group. The last row opens Project › Connections.
// Not drawn when the project has none.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { emit as emitEvent } from '@tauri-apps/api/event'
import { Cloud, CloudCog, Github, Gitlab, Plug, Settings2 } from 'lucide-vue-next'
import { DropdownMenu, DropdownItem, DropdownSeparator } from '@/components/ui'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute } from '@/lib/detachedScreens'
import { projectPageQuery } from '@/lib/projectSections'
import { connectionSummary } from '@/lib/sessionInfoBar'
import { INFO_CHIP } from './infoBarStyle'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const { router, projectId, isSessionWindow, connectionGroups } = useRunSession()

const summary = computed(() => {
  const count = (id: string) => connectionGroups.value.find((g) => g.id === id)?.rows.length ?? 0
  return connectionSummary({ git: count('git'), servers: count('servers'), aws: count('aws'), azure: count('azure') })
})

const title = computed(() =>
  t('run.infoBar.connections.title', {
    parts: summary.value.parts.map((p) => t(`run.infoBar.connections.parts.${p.kind}`, p.count)).join(', '),
  }),
)

// A session pop-out has no project screen: the main window opens it.
function editInProject() {
  const path = `/projects?${new URLSearchParams(projectPageQuery(projectId.value, 'github'))}`
  if (isSessionWindow) void emitEvent(SCREEN_OPEN_ROUTE, { path } satisfies ScreenOpenRoute)
  else void router.push(path)
}
</script>

<template>
  <DropdownMenu v-if="summary.total > 0" align="right" prefer="above">
    <template #trigger="{ open }">
      <button type="button" :class="INFO_CHIP" :title="title" :aria-label="title" aria-haspopup="menu" :aria-expanded="open">
        <Plug class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        <span class="tabular-nums">{{ summary.total }}</span>
      </button>
    </template>
    <div class="w-[20rem]">
      <div class="max-h-[24rem] overflow-y-auto">
        <template v-for="g in connectionGroups" :key="g.id">
          <div class="px-2.5 pb-0.5 pt-1.5 text-2xs font-medium text-muted-foreground/70">
            {{ t(`run.infoBar.connections.${g.id}`) }}
          </div>
          <div
            v-for="row in g.rows"
            :key="row.key"
            class="flex items-center gap-2 rounded px-2.5 py-1.5 text-sm text-popover-foreground"
            :title="row.title"
          >
            <Github v-if="row.provider === 'github'" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <Gitlab v-else-if="row.provider === 'gitlab'" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <span
              v-else-if="g.id === 'servers'"
              class="h-2 w-2 shrink-0 rounded-full"
              :class="row.status === 'online' ? 'bg-success' : row.status === 'offline' ? 'bg-destructive' : 'bg-muted-foreground/50'"
            />
            <Cloud v-else-if="g.id === 'aws'" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <CloudCog v-else class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <span class="min-w-0 flex-1 truncate">
              {{ row.label }}
              <template v-if="row.status === 'offline'"> · {{ t('run.infoBar.connections.offline') }}</template>
              <template v-else-if="row.isDefault"> · {{ t('run.infoBar.connections.isDefault') }}</template>
            </span>
            <span
              v-if="row.isReview"
              class="shrink-0 rounded border border-border px-1 text-2xs text-muted-foreground"
            >{{ t('run.infoBar.connections.review') }}</span>
            <span v-if="row.sub" class="shrink-0 text-2xs text-muted-foreground">{{ row.sub }}</span>
          </div>
        </template>
      </div>
      <DropdownSeparator />
      <DropdownItem @click="editInProject">
        <Settings2 class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        {{ t('run.infoBar.connections.edit') }}
      </DropdownItem>
    </div>
  </DropdownMenu>
</template>
