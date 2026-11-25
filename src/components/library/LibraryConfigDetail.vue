<script setup lang="ts">
// Right pane of the Library for a hook or a plugin (LH): what Claude Code's
// settings files declare, read-only. No edit, delete or toggle: the files are
// the user's and every session reads them, so the pane says where to change it
// and shows the file (or the plugin's folder) in Finder.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { FolderOpen } from 'lucide-vue-next'
import { Badge, Banner, Button } from '@/components/ui'
import DetailHeader from '@/components/listDetail/DetailHeader.vue'
import DetailSection from '@/components/listDetail/DetailSection.vue'
import { useToast } from '@/composables/useToast'
import { pluginPartGroups, pluginState, type ConfigItem } from '@/lib/libraryConfig'

const props = defineProps<{
  item: ConfigItem
}>()

const { t } = useI18n()
const { toast } = useToast()

const hook = computed(() => props.item.hook)
const plugin = computed(() => props.item.plugin)
const state = computed(() => (plugin.value ? pluginState(plugin.value) : null))
const parts = computed(() => (plugin.value?.parts ? pluginPartGroups(plugin.value.parts) : []))
const customPaths = computed(() => plugin.value?.parts?.custom_paths ?? [])

async function reveal(path: string) {
  try {
    await revealItemInDir(path)
  } catch (e) {
    toast.error(String(e))
  }
}

function revealInstall() {
  if (plugin.value?.install_path) void reveal(plugin.value.install_path)
}
</script>

<template>
  <section class="flex min-w-0 flex-1 flex-col">
    <DetailHeader :title="item.name">
      <template #meta>
        <span class="font-mono" :title="item.file">{{ item.file }}</span>
      </template>
      <template #actions>
        <Button variant="outline" size="sm" :title="item.file" @click="reveal(item.file)">
          <FolderOpen class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ t('library.config.reveal') }}
        </Button>
      </template>
    </DetailHeader>

    <div class="flex shrink-0 items-center gap-1.5 border-b border-border/60 bg-muted/30 px-3 py-1.5">
      <Badge size="xs" class="shrink-0">{{ t('library.engine.claude') }}</Badge>
      <Badge size="xs" class="shrink-0" :title="t(`library.config.sourceHint.${item.source}`)">
        {{ t(`library.config.source.${item.source}`) }}
      </Badge>
      <Badge
        v-if="state"
        size="xs"
        :tone="state === 'on' ? 'success' : 'neutral'"
        class="shrink-0"
        :title="t(`library.config.stateHint.${state}`)"
      >
        {{ t(`library.config.state.${state}`) }}
      </Badge>
      <Badge v-if="plugin && !plugin.installed" size="xs" tone="warning" class="shrink-0" :title="t('library.config.notInstalledHint')">
        {{ t('library.config.notInstalled') }}
      </Badge>
    </div>

    <Banner v-if="item.error" tone="destructive" class="m-4">{{ item.error }}</Banner>

    <div class="min-h-0 flex-1 overflow-auto px-5">
      <Banner tone="info" class="mt-4 text-xs">{{ t('library.config.readOnly') }}</Banner>

      <template v-if="hook">
        <Banner v-if="hook.all_disabled" tone="warning" class="mt-3 text-xs">{{ t('library.config.allDisabled') }}</Banner>
        <DetailSection :title="t('library.config.hookTitle')">
          <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-xs leading-relaxed">
            <dt class="text-muted-foreground">{{ t('library.config.event') }}</dt>
            <dd class="font-mono">{{ hook.event }}</dd>
            <dt class="text-muted-foreground">{{ t('library.config.matcher') }}</dt>
            <dd class="font-mono">{{ hook.matcher ?? t('library.config.anyMatcher') }}</dd>
            <dt class="text-muted-foreground">{{ t('library.config.type') }}</dt>
            <dd class="font-mono">{{ hook.hook_type || '—' }}</dd>
            <template v-if="hook.timeout !== null">
              <dt class="text-muted-foreground">{{ t('library.config.timeout') }}</dt>
              <dd>{{ t('library.config.seconds', { n: hook.timeout }) }}</dd>
            </template>
          </dl>
        </DetailSection>
        <DetailSection :title="t('library.config.runs')">
          <pre class="overflow-x-auto whitespace-pre-wrap break-all rounded-md border border-border bg-muted/40 p-3 font-mono text-xs">{{ hook.command }}</pre>
        </DetailSection>
      </template>

      <template v-if="plugin">
        <DetailSection :title="t('library.config.pluginTitle')">
          <p v-if="item.description" class="text-xs leading-relaxed">{{ item.description }}</p>
          <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-xs leading-relaxed">
            <dt class="text-muted-foreground">{{ t('library.config.key') }}</dt>
            <dd class="font-mono">{{ plugin.key }}</dd>
            <template v-if="plugin.marketplace">
              <dt class="text-muted-foreground">{{ t('library.config.marketplace') }}</dt>
              <dd class="font-mono">{{ plugin.marketplace }}</dd>
            </template>
            <template v-if="plugin.version">
              <dt class="text-muted-foreground">{{ t('library.config.version') }}</dt>
              <dd class="font-mono">{{ plugin.version }}</dd>
            </template>
            <template v-if="plugin.install_scope">
              <dt class="text-muted-foreground">{{ t('library.config.installScope') }}</dt>
              <dd class="font-mono">{{ plugin.install_scope }}</dd>
            </template>
            <template v-if="plugin.install_path">
              <dt class="text-muted-foreground">{{ t('library.config.installFolder') }}</dt>
              <dd class="flex min-w-0 items-center gap-1">
                <span class="truncate font-mono" :title="plugin.install_path">{{ plugin.install_path }}</span>
                <Button
                  variant="ghost"
                  size="icon"
                  class="shrink-0"
                  :title="t('library.config.reveal')"
                  :aria-label="t('library.config.reveal')"
                  @click="revealInstall"
                >
                  <FolderOpen class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
              </dd>
            </template>
          </dl>
        </DetailSection>

        <DetailSection v-if="plugin.parts" :title="t('library.config.parts')">
          <p v-if="parts.length === 0" class="text-xs text-muted-foreground">{{ t('library.config.noParts') }}</p>
          <div v-for="group in parts" :key="group.key" class="space-y-1">
            <div class="text-2xs text-muted-foreground">
              {{ t(`library.config.part.${group.key}`) }} ({{ group.names.length }})
            </div>
            <div class="flex flex-wrap gap-1">
              <Badge v-for="name in group.names" :key="name" size="xs" class="font-mono">{{ name }}</Badge>
            </div>
            <p v-if="group.key === 'mcp_servers'" class="text-2xs text-muted-foreground">{{ t('library.config.mcpNote') }}</p>
          </div>
          <p v-if="customPaths.length" class="text-2xs text-muted-foreground">
            {{ t('library.config.customPaths', { keys: customPaths.join(', ') }) }}
          </p>
        </DetailSection>
      </template>
    </div>
  </section>
</template>
