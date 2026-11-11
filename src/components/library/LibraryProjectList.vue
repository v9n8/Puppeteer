<script setup lang="ts">
// Left column of the Library's project tab (UX-54): search, then one row per
// project. Collapsed, it keeps only the project tiles, so switching stays one
// click while the items and the file get the width.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { PanelLeftClose, PanelLeftOpen, Search, X } from 'lucide-vue-next'
import { Button, Input } from '@/components/ui'
import { rowClass, rowListClass, rowSubClass, rowTitleClass } from '@/components/listDetail/rowStyles'
import { projectIcon, projectTileClass } from '@/lib/projectAppearance'
import { filterProjects } from '@/lib/projectList'
import type { Project } from '@/stores/projects'

const props = defineProps<{
  projects: Project[]
  selectedId: string | null
  loading: boolean
}>()

const collapsed = defineModel<boolean>('collapsed', { required: true })

const emit = defineEmits<{ select: [id: string] }>()

const { t } = useI18n()

const search = ref('')
const rows = computed(() => filterProjects(props.projects, search.value))
</script>

<template>
  <aside v-if="collapsed" class="flex w-11 shrink-0 flex-col items-center gap-1 border-r border-border/60 bg-sidebar py-2">
    <Button
      variant="ghost"
      size="icon-sm"
      :title="t('library.expandProjects')"
      :aria-label="t('library.expandProjects')"
      @click="collapsed = false"
    >
      <PanelLeftOpen class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
    <div class="min-h-0 w-full flex-1 space-y-1 overflow-y-auto px-1.5 pt-1">
      <button
        v-for="project in projects"
        :key="project.id"
        type="button"
        class="flex h-8 w-8 cursor-pointer items-center justify-center rounded-md border transition-shadow"
        :class="[projectTileClass(project.color), project.id === selectedId && 'ring-2 ring-primary']"
        :title="project.name"
        :aria-label="project.name"
        :aria-current="project.id === selectedId ? 'true' : undefined"
        @click="emit('select', project.id)"
      >
        <component :is="projectIcon(project.icon)" class="h-3.5 w-3.5" :stroke-width="1.75" />
      </button>
    </div>
  </aside>

  <aside v-else class="flex w-56 shrink-0 flex-col border-r border-border/60 bg-sidebar">
    <div class="flex shrink-0 items-center gap-1.5 border-b border-border/60 px-3 py-3">
      <div class="relative min-w-0 flex-1">
        <Search
          class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
          :stroke-width="1.75"
        />
        <Input v-model="search" class="pl-8 pr-7" :placeholder="t('projects.searchPlaceholder')" />
        <button
          v-if="search"
          type="button"
          class="absolute right-2 top-1/2 -translate-y-1/2 cursor-pointer text-muted-foreground hover:text-foreground"
          :title="t('common.clear')"
          @click="search = ''"
        >
          <X class="h-3.5 w-3.5" :stroke-width="2" />
        </button>
      </div>
      <Button
        variant="ghost"
        size="icon"
        class="shrink-0"
        :title="t('library.collapseProjects')"
        :aria-label="t('library.collapseProjects')"
        @click="collapsed = true"
      >
        <PanelLeftClose class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto px-2 py-2">
      <div v-if="loading && projects.length === 0" class="space-y-1 px-1">
        <div v-for="i in 4" :key="i" class="h-12 animate-pulse rounded-md bg-muted/60" />
      </div>
      <p v-else-if="projects.length === 0" class="px-3 py-8 text-center text-2xs text-muted-foreground">
        {{ t('library.noProjects') }}
      </p>
      <p v-else-if="rows.length === 0" class="px-3 py-8 text-center text-2xs text-muted-foreground">
        {{ t('common.noResults') }}
      </p>
      <ul v-else :class="rowListClass">
        <li v-for="project in rows" :key="project.id">
          <button
            type="button"
            class="cursor-pointer gap-2.5 pl-2 pr-1 text-left"
            :class="rowClass(project.id === selectedId)"
            :aria-current="project.id === selectedId ? 'true' : undefined"
            @click="emit('select', project.id)"
          >
            <span
              class="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
              :class="projectTileClass(project.color)"
            >
              <component :is="projectIcon(project.icon)" class="h-3.5 w-3.5" :stroke-width="1.75" />
            </span>
            <span class="flex min-w-0 flex-1 flex-col">
              <span class="truncate" :class="rowTitleClass" :title="project.name">{{ project.name }}</span>
              <span class="font-mono" :class="rowSubClass" :title="project.path">{{ project.path }}</span>
            </span>
          </button>
        </li>
      </ul>
    </div>
  </aside>
</template>
