<script setup lang="ts">
// Turn links into files (UX-58, D1 step 2): every link Puppeteer made into its
// old library, in HOME and every project, listed before anything changes —
// each becomes a copy of its own, and Puppeteer's blocks in a project's
// AGENTS.md lose their markers (their text stays). The apply takes back the
// paths listed here, so a link that shows up later is not touched; it stops at
// the first failure and what it did stays done. Local-only: the Library hides
// the button on a remote desktop.
import { computed, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RotateCw } from 'lucide-vue-next'
import { Badge, Banner, Button, Modal } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import {
  CMD_LIBRARY_MATERIALIZE, CMD_LIBRARY_MATERIALIZE_PREVIEW, materializeCounts, materializePaths,
  type MaterializeOutcome, type PlannedPlace,
} from '@/lib/libraryDisk'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [value: boolean]; changed: [] }>()

const { t } = useI18n()

const plan = shallowRef<PlannedPlace[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const applying = ref(false)
const applyError = ref<string | null>(null)
const outcome = shallowRef<MaterializeOutcome | null>(null)

const paths = computed(() => materializePaths(plan.value))
const counts = computed(() => materializeCounts(plan.value))

async function load() {
  loading.value = true
  try {
    plan.value = await invoke<PlannedPlace[]>(CMD_LIBRARY_MATERIALIZE_PREVIEW)
    error.value = null
  } catch (e) {
    plan.value = []
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

watch(
  () => props.open,
  (open) => {
    if (!open) return
    outcome.value = null
    applyError.value = null
    void load()
  },
  { immediate: true },
)

async function apply() {
  applying.value = true
  applyError.value = null
  try {
    outcome.value = await invoke<MaterializeOutcome>(CMD_LIBRARY_MATERIALIZE, { paths: paths.value })
    if (outcome.value.done.length > 0) emit('changed')
  } catch (e) {
    applyError.value = String(e)
  } finally {
    applying.value = false
  }
  // What is left (after a failure, or links made meanwhile) shows again.
  await load()
}

function placeLabel(place: PlannedPlace): string {
  return place.scope === 'global' ? t('library.materialize.home') : (place.project_name ?? place.root)
}
</script>

<template>
  <Modal
    :open="open"
    size="2xl"
    scroll-body
    :closable="!applying"
    :dismiss-on-overlay="false"
    @update:open="emit('update:open', $event)"
  >
    <template #header>
      <h3 class="flex-1 text-sm font-semibold">{{ t('library.materialize.title') }}</h3>
      <Button
        variant="ghost"
        size="icon"
        :disabled="loading || applying"
        :title="t('common.refresh')"
        :aria-label="t('common.refresh')"
        @click="load"
      >
        <RotateCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" :stroke-width="1.75" />
      </Button>
    </template>

    <div class="space-y-4 p-4">
      <p class="whitespace-pre-line text-xs text-muted-foreground">{{ t('library.materialize.intro') }}</p>

      <Banner v-if="error" tone="destructive">{{ error }}</Banner>
      <Banner v-if="applyError" tone="destructive" live>{{ applyError }}</Banner>

      <Banner v-if="outcome" :tone="outcome.failed ? 'warning' : 'success'" live>
        <p>{{ t('library.materialize.done', { count: outcome.done.length }) }}</p>
        <p v-if="outcome.failed" class="mt-1 break-all">
          {{ t('library.materialize.failed', { path: outcome.failed.path, error: outcome.failed.error }) }}
        </p>
        <p v-if="outcome.skipped.length > 0" class="mt-1">
          {{ t('library.materialize.skipped', { count: outcome.skipped.length }) }}
        </p>
      </Banner>

      <p v-if="loading && plan.length === 0" class="text-xs text-muted-foreground">{{ t('library.materialize.loading') }}</p>
      <p v-else-if="!error && plan.length === 0" class="text-xs text-muted-foreground">{{ t('library.materialize.nothing') }}</p>

      <Banner v-if="counts.inGit > 0" tone="warning">{{ t('library.materialize.inGitWarning', { count: counts.inGit }) }}</Banner>

      <div
        v-for="place in plan"
        :key="place.root"
        class="rounded-lg border border-border bg-card"
        :class="(loading || applying) && 'opacity-60'"
      >
        <div class="flex min-w-0 items-baseline gap-2 border-b border-border/50 px-3 py-2 text-xs">
          <span class="shrink-0 font-semibold">{{ placeLabel(place) }}</span>
          <span class="min-w-0 truncate font-mono text-2xs text-muted-foreground" :title="place.root">{{ place.root }}</span>
        </div>
        <ul class="divide-y divide-border/40">
          <li
            v-for="link in place.links"
            :key="link.entry"
            class="space-y-0.5 px-3 py-2 text-xs"
            :class="link.error && 'opacity-70'"
          >
            <div class="flex min-w-0 items-center gap-1.5">
              <Badge size="xs" class="shrink-0">{{ t(`library.kind.${link.kind}`) }}</Badge>
              <Badge size="xs" class="shrink-0">{{ t(`library.engine.${link.engine}`) }}</Badge>
              <span class="min-w-0 truncate font-mono font-medium" :title="link.entry">{{ link.entry }}</span>
              <Badge
                v-if="link.git_tracked"
                size="xs"
                tone="warning"
                class="shrink-0"
                :title="t('library.statusHint.git_tracked')"
              >
                {{ t('library.status.git_tracked') }}
              </Badge>
            </div>
            <p class="truncate font-mono text-2xs text-muted-foreground" :title="link.target">
              {{ t('library.materialize.copiedFrom', { target: link.target }) }}
            </p>
            <p v-if="link.error" class="break-all text-2xs text-destructive">
              {{ t('library.materialize.leftAsIs', { error: link.error }) }}
            </p>
          </li>
          <li v-if="place.agents_md" class="space-y-0.5 px-3 py-2 text-xs" :class="place.agents_md.error && 'opacity-70'">
            <div class="flex min-w-0 items-center gap-1.5">
              <Badge size="xs" class="shrink-0">{{ t('library.kind.memory') }}</Badge>
              <span class="min-w-0 truncate font-mono font-medium" :title="place.agents_md.path">{{ place.agents_md.path }}</span>
              <Badge
                v-if="place.agents_md.git_tracked"
                size="xs"
                tone="warning"
                class="shrink-0"
                :title="t('library.statusHint.git_tracked')"
              >
                {{ t('library.status.git_tracked') }}
              </Badge>
            </div>
            <p class="break-all text-2xs text-muted-foreground">
              {{ t('library.materialize.blocks', { names: place.agents_md.blocks.join(', ') }) }}
            </p>
            <p v-if="place.agents_md.error" class="break-all text-2xs text-destructive">
              {{ t('library.materialize.leftAsIs', { error: place.agents_md.error }) }}
            </p>
          </li>
        </ul>
      </div>
    </div>

    <template #footer>
      <span class="mr-auto text-2xs text-muted-foreground">{{ t('library.materialize.libraryKept') }}</span>
      <Button variant="ghost" :disabled="applying" @click="emit('update:open', false)">{{ t('common.close') }}</Button>
      <Button :disabled="paths.length === 0 || loading || applying" @click="apply">
        {{ applying ? t('library.materialize.applying') : t('library.materialize.apply', { count: counts.changes }) }}
      </Button>
    </template>
  </Modal>
</template>
