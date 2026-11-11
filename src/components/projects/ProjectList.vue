<script setup lang="ts">
// Left column of the Projects page (UX-49): search, the hidden-projects
// toggle, "Add project", then one row per project. A click only selects the
// project — sessions open from the titlebar tabs (D3). The grip drags a row to
// reorder. Same look as listDetail/ListDetailSidebar, which cannot drag.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Eye, EyeOff, GripVertical, Plus, RotateCw, Search, X } from 'lucide-vue-next'
import { Badge, Banner, Button, Input } from '@/components/ui'
import { rowClass, rowListClass, rowSubClass, rowTitleClass } from '@/components/listDetail/rowStyles'
import { useDragSort } from '@/composables/useDragSort'
import { projectIcon, projectTileClass } from '@/lib/projectAppearance'
import { filterProjects } from '@/lib/projectList'
import type { Project } from '@/stores/projects'

const props = defineProps<{
  /** The projects to list (hidden ones already left out unless shown). */
  projects: Project[]
  selectedId: string | null
  hiddenCount: number
  /** True when there is no project at all (vs. all hidden / none matching). */
  emptyStore: boolean
  loading: boolean
  error: string | null
  addLabel: string
  addDisabled: boolean
}>()

const search = defineModel<string>('search', { required: true })
const showHidden = defineModel<boolean>('showHidden', { required: true })

const emit = defineEmits<{
  select: [id: string]
  add: []
  retry: []
  /** Drop `fromId` on the slot of `toId`. */
  move: [fromId: string, toId: string]
}>()

const { t } = useI18n()

const rows = computed(() => filterProjects(props.projects, search.value))

const listRef = ref<HTMLElement | null>(null)
const { dragFrom, start: startDrag, itemStyle, clickWasDrag } = useDragSort({
  container: listRef,
  onDrop: (from, to) => emit('move', rows.value[from].id, rows.value[to].id),
})

/** The click that ends a drag is the drop, not a pick: swallow it before any row sees it. */
function onListClickCapture(e: MouseEvent) {
  if (clickWasDrag()) e.stopPropagation()
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-sidebar">
    <div class="shrink-0 space-y-2 border-b border-border/60 px-3 py-3">
      <div class="flex items-center gap-1.5">
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
          v-if="hiddenCount > 0"
          :variant="showHidden ? 'primary' : 'outline'"
          size="icon"
          class="shrink-0"
          :aria-pressed="showHidden"
          :title="t('projects.showHidden', { count: hiddenCount })"
          :aria-label="t('projects.showHidden', { count: hiddenCount })"
          @click="showHidden = !showHidden"
        >
          <component :is="showHidden ? Eye : EyeOff" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
      </div>
      <Button size="sm" class="w-full" :disabled="addDisabled" @click="emit('add')">
        <Plus class="h-3.5 w-3.5" :stroke-width="2" />
        {{ addLabel }}
      </Button>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto px-2 py-2">
      <Banner v-if="error" tone="destructive" class="mb-2">
        {{ error }}
        <template #actions>
          <Button
            variant="ghost"
            size="icon-xs"
            :title="t('common.refresh')"
            :aria-label="t('common.refresh')"
            @click="emit('retry')"
          >
            <RotateCw class="h-3 w-3" :stroke-width="2" />
          </Button>
        </template>
      </Banner>

      <div v-if="loading && projects.length === 0" class="space-y-1 px-1">
        <div v-for="i in 5" :key="i" class="h-12 animate-pulse rounded-md bg-muted/60" />
      </div>
      <div v-else-if="emptyStore && !error" class="px-3 py-8 text-center">
        <p class="text-xs text-muted-foreground">{{ t('projects.emptyTitle') }}</p>
        <p class="mt-1 text-2xs text-muted-foreground/70">{{ t('projects.emptyHint') }}</p>
      </div>
      <div v-else-if="projects.length === 0 && !error" class="px-3 py-8 text-center">
        <p class="text-xs text-muted-foreground">{{ t('projects.allHiddenTitle') }}</p>
        <Button class="mt-3" variant="outline" size="xs" @click="showHidden = true">
          <Eye class="h-3 w-3" :stroke-width="2" />
          {{ t('projects.showHidden', { count: hiddenCount }) }}
        </Button>
      </div>
      <p v-else-if="rows.length === 0 && !error" class="px-3 py-8 text-center text-2xs text-muted-foreground">
        {{ t('common.noResults') }}
      </p>

      <ul v-else ref="listRef" :class="rowListClass" @click.capture="onListClickCapture">
        <li
          v-for="(project, index) in rows"
          :key="project.id"
          :data-drag-index="index"
          :class="dragFrom === index && 'relative rounded-md bg-card shadow-xl ring-1 ring-primary/40'"
          :style="itemStyle(index)"
        >
          <div class="group/row gap-1 pl-2 pr-1" :class="rowClass(project.id === selectedId)">
            <button
              type="button"
              class="flex min-w-0 flex-1 cursor-pointer items-start gap-2.5 text-left"
              :class="{ 'opacity-60': project.hidden }"
              :aria-current="project.id === selectedId ? 'true' : undefined"
              @click="emit('select', project.id)"
            >
              <span
                class="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
                :class="projectTileClass(project.color)"
              >
                <component :is="projectIcon(project.icon)" class="h-3.5 w-3.5" :stroke-width="1.75" />
              </span>
              <span class="min-w-0 flex-1">
                <span class="flex min-w-0 items-center gap-1.5">
                  <span class="truncate" :class="rowTitleClass" :title="project.name">{{ project.name }}</span>
                  <Badge v-if="project.hidden" tone="neutral" class="shrink-0">
                    <EyeOff class="h-2.5 w-2.5" :stroke-width="2" />
                    {{ t('projects.hiddenBadge') }}
                  </Badge>
                </span>
                <span class="block font-mono" :class="rowSubClass" :title="project.path">{{ project.path }}</span>
              </span>
            </button>
            <button
              type="button"
              class="flex h-6 w-6 shrink-0 cursor-grab touch-none items-center justify-center self-center rounded-md text-muted-foreground/60 transition-opacity hover:bg-accent hover:text-foreground focus-visible:opacity-100 group-hover/row:opacity-100 active:cursor-grabbing"
              :class="dragFrom === index ? 'opacity-100' : 'opacity-0'"
              :title="t('projects.dragToReorder')"
              :aria-label="t('projects.dragToReorder')"
              @pointerdown="startDrag(index, $event)"
            >
              <GripVertical class="h-3.5 w-3.5" :stroke-width="1.75" />
            </button>
          </div>
        </li>
      </ul>
    </div>
  </div>
</template>
