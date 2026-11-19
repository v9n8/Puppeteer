<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { open as pickFolder } from '@tauri-apps/plugin-dialog'
import { Eye, EyeOff, FolderPlus, Plus, Search } from 'lucide-vue-next'
import { Button, DropdownLabel } from '@/components/ui'
import AddProjectModal from '@/components/projects/AddProjectModal.vue'
import { useProjectsStore, type DetectedProjectInfo, type DetectedRepo } from '@/stores/projects'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { useToast } from '@/composables/useToast'
import { useProjectSwitcher } from '@/composables/useProjectSwitcher'
import { IS_MAC, effectiveKeymap } from '@/lib/appMenu'
import { formatAccelerator } from '@/lib/keymap'
import { projectIcon, projectTextClass } from '@/lib/projectAppearance'
import { stepIndex, switcherRows, wasRestored } from '@/lib/projectSwitcher'

/**
 * Titlebar button next to the project tabs (UX-09). Its popover finds a
 * registered project by name or path and opens it as a tab, or starts a
 * session in it; its footer registers a new folder as a project. Enter opens
 * the highlighted row (the first one by default), the arrows move, Esc closes.
 */

const { t } = useI18n()
const { toast } = useToast()
const projects = useProjectsStore()
const tabs = useWorkspaceTabsStore()
const { open, currentProjectId, openProject } = useProjectSwitcher()

const PANEL_WIDTH = 400
const EDGE_MARGIN = 8

const trigger = ref<HTMLElement | null>(null)
const panel = ref<HTMLElement | null>(null)
const search = ref<HTMLInputElement | null>(null)
const list = ref<HTMLElement | null>(null)
const pos = ref({ x: 0, y: 0 })

const query = ref('')
const active = ref(0)
const showHidden = ref(false)

const rows = computed(() =>
  switcherRows(projects.projects, { query: query.value, mru: tabs.recent, showHidden: showHidden.value }),
)
const hiddenCount = computed(() => projects.projects.filter((p) => p.hidden).length)
const hasRecent = computed(() => rows.value.some((r) => r.recent))

const shortcut = computed(() => {
  const accel = effectiveKeymap().get('file.openProject')
  return accel ? ` (${formatAccelerator(accel, IS_MAC)})` : ''
})

// ── Adding a folder ───────────────────────────────────────────────────────────
const detecting = ref(false)
const adding = ref(false)
const pending = ref<DetectedProjectInfo | null>(null)

async function startAdd() {
  const selected = await pickFolder({ directory: true, multiple: false, title: t('projects.selectFolderTitle') })
  if (!selected || Array.isArray(selected)) return
  // Already registered: nothing to add, just open it.
  const existing = projects.projects.find((p) => p.path === selected)
  if (existing) {
    toast.info(t('projectSwitcher.alreadyAdded', { name: existing.name }))
    choose(existing.id)
    return
  }
  detecting.value = true
  try {
    const info = await projects.detectProjectInfo(selected)
    pending.value = info
    // The modal sits above the page, not inside the popover.
    close()
  } catch (e) {
    toast.error(String(e))
  } finally {
    detecting.value = false
  }
}

function cancelAdd() {
  pending.value = null
}

async function confirmAdd({ name, repos }: { name: string; repos: DetectedRepo[] }) {
  const info = pending.value
  if (!info || adding.value) return
  adding.value = true
  const startedAt = Date.now()
  try {
    const project = await projects.addProject({
      path: info.path,
      name: name || undefined,
      repos:
        repos.length > 0
          ? repos.map((r) => ({
              name: r.name,
              path: r.path,
              provider: r.provider,
              github_owner: r.github_owner || undefined,
              github_repo: r.github_repo || undefined,
              gitlab_project_path: r.gitlab_project_path || null,
            }))
          : undefined,
    })
    // A path whose project sits in the Trash brings it back, sessions and all (UX-06).
    if (wasRestored(project, startedAt)) toast.success(t('projectSwitcher.restored', { name: project.name }))
    else toast.success(t('projects.toastAdded'))
    pending.value = null
    choose(project.id)
  } catch (e) {
    toast.error(String(e).includes('project_is_home') ? t('projects.errorHomeFolder') : String(e))
  } finally {
    adding.value = false
  }
}

// ── Popover ──────────────────────────────────────────────────────────────────
function place() {
  const anchor = trigger.value?.getBoundingClientRect()
  if (!anchor) return
  const width = Math.min(PANEL_WIDTH, window.innerWidth - EDGE_MARGIN * 2)
  const x = Math.max(EDGE_MARGIN, Math.min(anchor.left, window.innerWidth - width - EDGE_MARGIN))
  pos.value = { x, y: anchor.bottom + 6 }
}

function close() {
  open.value = false
}

function choose(projectId: string) {
  close()
  openProject(projectId)
}

function onDocPointer(e: Event) {
  const target = e.target
  if (!(target instanceof Node)) return
  if (panel.value?.contains(target) || trigger.value?.contains(target)) return
  close()
}

function onDocKey(e: Event) {
  if ((e as KeyboardEvent).key === 'Escape') {
    e.preventDefault()
    close()
  }
}

function bind(on: boolean) {
  const m = on ? 'addEventListener' : 'removeEventListener'
  document[m]('mousedown', onDocPointer)
  document[m]('keydown', onDocKey)
  window[m]('resize', place)
}

watch(open, async (isOpen) => {
  bind(isOpen)
  if (!isOpen) return
  query.value = ''
  active.value = 0
  place()
  if (projects.projects.length === 0) void projects.fetchProjects()
  await nextTick()
  search.value?.focus()
})

watch(query, () => {
  active.value = 0
})

watch(active, async (i) => {
  await nextTick()
  list.value?.querySelector<HTMLElement>(`[data-row="${i}"]`)?.scrollIntoView({ block: 'nearest' })
})

function onSearchKey(e: KeyboardEvent) {
  if (e.isComposing) return
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    active.value = stepIndex(active.value, e.key === 'ArrowDown' ? 1 : -1, rows.value.length)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    const row = rows.value[active.value] ?? rows.value[0]
    if (row) choose(row.project.id)
  }
}

// Focus mode drops the titlebar: don't come back with a popover nobody placed.
onBeforeUnmount(() => {
  bind(false)
  open.value = false
})
</script>

<template>
  <div ref="trigger" class="shrink-0">
    <Button
      variant="ghost"
      size="icon-sm"
      :class="open ? 'bg-accent text-foreground' : ''"
      :title="t('projectSwitcher.button') + shortcut"
      :aria-label="t('projectSwitcher.button')"
      :aria-expanded="open"
      aria-haspopup="dialog"
      @click="open = !open"
    >
      <Plus class="h-4 w-4" :stroke-width="1.75" />
    </Button>
  </div>

  <Teleport to="body">
    <transition
      enter-active-class="transition-opacity duration-150 ease-out"
      enter-from-class="opacity-0"
      leave-active-class="transition-opacity duration-100 ease-in"
      leave-to-class="opacity-0"
    >
      <div
        v-if="open"
        ref="panel"
        role="dialog"
        :aria-label="t('projectSwitcher.button')"
        class="ui-popover fixed z-[100] flex max-h-[min(480px,calc(100vh-64px))] flex-col rounded-card border border-popover-border bg-popover"
        :style="{ left: `${pos.x}px`, top: `${pos.y}px`, width: `min(${PANEL_WIDTH}px, calc(100vw - ${EDGE_MARGIN * 2}px))` }"
      >
        <!-- Search: sticky above the list -->
        <div class="flex items-center gap-2 border-b border-border px-3 py-2">
          <Search class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="2" />
          <input
            ref="search"
            v-model="query"
            type="text"
            class="h-6 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            :placeholder="t('projectSwitcher.searchPlaceholder')"
            :aria-label="t('projectSwitcher.searchPlaceholder')"
            @keydown="onSearchKey"
          />
        </div>

        <!-- Rows -->
        <div ref="list" class="min-h-0 flex-1 overflow-y-auto p-1" role="listbox">
          <p v-if="rows.length === 0" class="px-2 py-6 text-center text-xs text-muted-foreground">
            {{ projects.projects.length === 0 ? t('projectSwitcher.noProjects') : t('projectSwitcher.noMatch') }}
          </p>
          <template v-for="(row, index) in rows" :key="row.project.id">
            <DropdownLabel
              v-if="hasRecent && (index === 0 || row.recent !== rows[index - 1].recent)"
              class="px-2 pb-1 pt-2"
            >
              {{ row.recent ? t('projectSwitcher.recent') : t('projectSwitcher.allProjects') }}
            </DropdownLabel>
            <div
              :data-row="index"
              role="option"
              :aria-selected="index === active"
              class="group flex min-h-11 cursor-pointer items-center gap-2.5 rounded-sm px-2.5 py-1"
              :class="index === active ? 'bg-accent text-foreground' : 'text-foreground/90 hover:bg-accent'"
              @mousemove="active = index"
              @click="choose(row.project.id)"
            >
              <component
                :is="projectIcon(row.project.icon)"
                class="h-4 w-4 shrink-0"
                :class="projectTextClass(row.project.color) || 'text-muted-foreground'"
                :stroke-width="1.75"
              />
              <div class="flex min-w-0 flex-1 flex-col">
                <span class="flex items-center gap-1.5 truncate text-sm">
                  <span class="truncate" :class="row.project.id === currentProjectId ? 'font-medium' : ''">
                    {{ row.project.name }}
                  </span>
                  <EyeOff
                    v-if="row.project.hidden"
                    class="h-3 w-3 shrink-0 text-muted-foreground"
                    :stroke-width="2"
                    :aria-label="t('projectSwitcher.hiddenBadge')"
                  />
                </span>
                <span v-if="row.showPath" class="truncate text-2xs text-muted-foreground" :title="row.project.path">
                  {{ row.project.path }}
                </span>
              </div>
              <!-- Out of the flow until hover, so it never takes the name's width -->
              <Button
                variant="outline"
                size="xs"
                class="hidden shrink-0 hover:bg-foreground/10 group-hover:flex focus-visible:flex"
                :class="index === active ? '!flex' : ''"
                @click.stop="choose(row.project.id)"
              >
                {{ t('projectSwitcher.open') }}
              </Button>
            </div>
          </template>
        </div>

        <!-- Footer: add a folder (its modal is below) + show hidden -->
        <div class="border-t border-border p-1">
          <div class="flex items-center gap-1">
            <Button variant="ghost" size="xs" class="flex-1 justify-start" :disabled="detecting" @click="startAdd">
              <FolderPlus class="h-3.5 w-3.5" :stroke-width="2" />
              {{ detecting ? t('projects.detecting') : t('projectSwitcher.addProject') }}
            </Button>
            <Button
              v-if="hiddenCount > 0"
              :variant="showHidden ? 'subtle' : 'ghost'"
              size="xs"
              :aria-pressed="showHidden"
              @click="showHidden = !showHidden"
            >
              <component :is="showHidden ? Eye : EyeOff" class="h-3.5 w-3.5" :stroke-width="2" />
              {{ t('projects.showHidden', { count: hiddenCount }) }}
            </Button>
          </div>
        </div>
      </div>
    </transition>
  </Teleport>

  <AddProjectModal :pending="pending" :busy="adding" @cancel="cancelAdd" @confirm="confirmAdd" />
</template>
