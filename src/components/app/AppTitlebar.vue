<script setup lang="ts">
// The main window's title bar. Shown / hidden by App.vue (focus mode hides it).
import { useI18n } from 'vue-i18n'
import { PanelLeft } from 'lucide-vue-next'
import { useUILayoutStore } from '@/stores/uiLayout'
import WorkspaceTabs from '@/components/WorkspaceTabs.vue'
import ProjectSwitcher from '@/components/ProjectSwitcher.vue'
import { Button } from '@/components/ui'

const uiLayout = useUILayoutStore()
const { t } = useI18n()
</script>

<template>
  <header
    class="app-titlebar flex items-center gap-2 h-11 px-3 shrink-0 bg-sidebar border-b border-border/50"
  >
    <img src="/logo.png" alt="Puppeteer" class="h-[18px] w-[18px] rounded shrink-0" />
    <p class="text-sm font-semibold leading-none tracking-tight shrink-0">Puppeteer</p>
    <div class="h-4 w-px bg-border shrink-0 mx-1" aria-hidden="true" />

    <WorkspaceTabs />
    <!-- Right after the last tab: it adds projects to the strip (UX-09). -->
    <ProjectSwitcher />

    <!-- Sits here rather than inside the sidebar so it stays put when the
         sidebar shrinks to the icon rail. -->
    <Button
      variant="ghost"
      size="icon-sm"
      class="shrink-0 ml-auto"
      :title="uiLayout.sidebarMode === 'rail' ? t('menu.showSidebar') : t('menu.hideSidebar')"
      :aria-label="uiLayout.sidebarMode === 'rail' ? t('menu.showSidebar') : t('menu.hideSidebar')"
      @click="uiLayout.toggleSidebar()"
    >
      <PanelLeft class="h-4 w-4" :stroke-width="1.75" />
    </Button>
  </header>
</template>
