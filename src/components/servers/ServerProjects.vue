<script setup lang="ts">
// "Projects using it" block of a server's detail (V3-52): the live Projects that
// switched this server on, read fresh whenever the server changes.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { ArrowUpRight } from 'lucide-vue-next'
import { Banner } from '@/components/ui'
import DetailSection from '@/components/listDetail/DetailSection.vue'
import { projectIcon, projectTileClass } from '@/lib/projectAppearance'
import { projectPageQuery } from '@/lib/projectSections'
import { useServersStore, type ServerProject } from '@/stores/servers'

const props = defineProps<{ serverId: string }>()

const { t } = useI18n()
const router = useRouter()
const store = useServersStore()

const projects = ref<ServerProject[]>([])
const error = ref<string | null>(null)
const loading = ref(true)

async function load(id: string) {
  loading.value = true
  error.value = null
  try {
    const list = await store.listProjectsOf(id)
    if (id === props.serverId) projects.value = list
  } catch (e) {
    if (id !== props.serverId) return
    projects.value = []
    error.value = String(e)
  } finally {
    if (id === props.serverId) loading.value = false
  }
}

watch(() => props.serverId, load, { immediate: true })

function open(projectId: string) {
  router.push({ path: '/projects', query: projectPageQuery(projectId, 'deploy') }).catch(() => {})
}
</script>

<template>
  <DetailSection :title="t('servers.projects.title')" :hint="t('servers.projects.hint')">
    <Banner v-if="error" tone="destructive">{{ t('servers.projects.loadFailed', { error }) }}</Banner>
    <p v-else-if="!loading && projects.length === 0" class="text-2xs text-muted-foreground">
      {{ t('servers.projects.none') }}
    </p>
    <ul v-else class="space-y-1.5">
      <li
        v-for="project in projects"
        :key="project.id"
        class="flex min-w-0 items-center gap-2.5 rounded-md border border-border/60 px-2.5 py-1.5"
      >
        <span
          class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
          :class="projectTileClass(project.color)"
        >
          <component :is="projectIcon(project.icon)" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </span>
        <span class="min-w-0 flex-1">
          <span class="flex min-w-0 items-center gap-1.5">
            <span class="truncate text-sm font-medium" :title="project.name">{{ project.name }}</span>
            <span
              v-if="project.roles.length > 1"
              class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-2xs text-muted-foreground"
            >
              {{ project.roles.join(', ') }}
            </span>
          </span>
          <span class="block truncate font-mono text-2xs text-muted-foreground/80" :title="project.path">
            {{ project.path }}
          </span>
        </span>
        <button
          type="button"
          class="flex h-7 shrink-0 cursor-pointer items-center gap-1 rounded-md px-2 text-2xs text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          @click="open(project.id)"
        >
          {{ t('servers.projects.open') }}
          <ArrowUpRight class="h-3 w-3" :stroke-width="1.75" />
        </button>
      </li>
    </ul>
  </DetailSection>
</template>
