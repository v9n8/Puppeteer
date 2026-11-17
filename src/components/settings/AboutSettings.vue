<script setup lang="ts">
// Settings › System › About: the app card (logo, version, stack), the Updates
// card (a manual check beside the 12-hour one; shares its state with the corner
// toast through composables/useUpdateCheck) and the What's new card (the bundled
// release notes, lib/whatsNew). `?release=<version>` opens that release and
// scrolls to it (the launch toast's "See what's new").
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { getVersion } from '@tauri-apps/api/app'
import { ChevronRight, Globe, RefreshCw, Sparkles } from 'lucide-vue-next'
import { Badge, Button, Card } from '@/components/ui'
import { useFeaturesStore } from '@/stores/features'
import { useUpdateCheck } from '@/composables/useUpdateCheck'
import { isRemoteMode } from '@/lib/ipc/remote'
import { updateLine } from '@/lib/updateStatus'
import { RELEASES, kindCounts, noteText, type Release } from '@/lib/whatsNew'

const { t, locale } = useI18n()
const props = defineProps<{ release?: string }>()
const features = useFeaturesStore()
const { state, lastCheckedAt, installing, check, install } = useUpdateCheck()

// Read from Tauri so it always matches the packaged build (tauri.conf.json).
const appVersion = ref('')
onMounted(async () => {
  try {
    appVersion.value = await getVersion()
  } catch {
    // Ignore (e.g. running outside the Tauri shell during dev in a browser).
  }
})

// The remote-desktop window shows the other machine; it cannot update from here.
const showUpdates = !isRemoteMode()

const line = computed(() =>
  updateLine(
    { ...state.value, enabled: features.isOn('app.auto_update'), installing: installing.value, lastCheckedAt: lastCheckedAt.value },
    t,
    (ms) => new Date(ms).toLocaleTimeString(locale.value, { hour: '2-digit', minute: '2-digit' }),
  ),
)

// The newest release is open, older ones folded; the toast's link opens its own.
const releaseKey = (r: Release) => r.version || 'next'
const openKeys = ref<Set<string>>(new Set(RELEASES.slice(0, 1).map(releaseKey)))
function toggle(r: Release) {
  const next = new Set(openKeys.value)
  if (!next.delete(releaseKey(r))) next.add(releaseKey(r))
  openKeys.value = next
}

watch(
  () => props.release,
  async (v) => {
    if (typeof v !== 'string') return
    const hit = RELEASES.find((r) => r.version === v)
    if (hit) openKeys.value = new Set([...openKeys.value, releaseKey(hit)])
    await nextTick()
    const target = (hit && document.getElementById(`release-${releaseKey(hit)}`)) || document.getElementById('about-whats-new')
    target?.scrollIntoView({ block: 'start', behavior: 'smooth' })
  },
  { immediate: true },
)

function summary(r: Release): string {
  return kindCounts(r).map((c) => t(`whatsNew.count.${c.kind}`, { n: c.count })).join(' · ')
}
</script>

<template>
  <div class="space-y-4">
    <!-- The app -->
    <Card body-class="flex flex-col items-center text-center p-6" data-settings-id="about-version">
      <img src="/logo.png" alt="Puppeteer" class="h-14 w-14 rounded-2xl mb-4" />
      <h2 class="text-xl font-bold tracking-tight">Puppeteer</h2>
      <p class="text-sm text-muted-foreground mt-1">{{ t('about.tagline') }}</p>
      <Badge v-if="appVersion" class="mt-2 px-2 py-0.5 text-2xs font-mono font-medium">v{{ appVersion }}</Badge>
      <p class="mt-4 max-w-md text-xs text-muted-foreground leading-relaxed">{{ t('about.description') }}</p>
      <div class="mt-5 flex items-center gap-3 text-xs text-muted-foreground">
        <span class="flex items-center gap-1">
          <Globe class="h-3 w-3" :stroke-width="1.5" />
          Tauri 2
        </span>
        <span class="h-1 w-1 rounded-full bg-border" />
        <span>Vue 3</span>
        <span class="h-1 w-1 rounded-full bg-border" />
        <span>Rust</span>
      </div>
    </Card>

    <!-- Updates -->
    <Card v-if="showUpdates" body-class="flex items-center gap-3 p-4" data-settings-id="about-updates">
      <template #header>
        <RefreshCw class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('about.updates') }}</span>
      </template>
      <p class="min-w-0 flex-1 text-sm text-foreground" role="status">{{ line.text }}</p>
      <Button
        v-if="line.action === 'check' || line.action === 'checking' || line.action === 'retry'"
        variant="outline"
        :disabled="line.action === 'checking'"
        @click="check()"
      >
        {{ line.action === 'checking' ? t('about.checking') : line.action === 'retry' ? t('about.retry') : t('about.check') }}
      </Button>
      <Button v-else-if="line.action === 'install' || line.action === 'installing'" :disabled="line.action === 'installing'" @click="install()">
        {{ line.action === 'installing' ? t('update.installing') : t('update.install') }}
      </Button>
    </Card>

    <!-- What's new -->
    <Card id="about-whats-new" body-class="divide-y divide-border/60" data-settings-id="about-whatsNew">
      <template #header>
        <Sparkles class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('about.whatsNew') }}</span>
      </template>
      <p v-if="RELEASES.length === 0" class="p-4 text-sm text-muted-foreground">{{ t('about.noNotes') }}</p>
      <section v-for="r in RELEASES" :id="`release-${releaseKey(r)}`" :key="releaseKey(r)">
        <button
          type="button"
          class="flex w-full min-h-[44px] items-center gap-2 px-4 py-2.5 text-left hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          :aria-expanded="openKeys.has(releaseKey(r))"
          @click="toggle(r)"
        >
          <ChevronRight
            class="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform"
            :class="openKeys.has(releaseKey(r)) && 'rotate-90'"
            :stroke-width="2"
          />
          <span class="text-sm font-semibold text-foreground">
            {{ r.version ? t('about.release', { version: r.version }) : t('about.nextRelease') }}
          </span>
          <Badge v-if="r.version && r.version === appVersion" tone="primary" size="xs">{{ t('about.current') }}</Badge>
          <span class="text-xs text-muted-foreground tabular-nums">{{ r.date }}</span>
          <span v-if="!openKeys.has(releaseKey(r))" class="ml-auto text-xs text-muted-foreground">{{ summary(r) }}</span>
        </button>
        <!-- Two columns: the label column takes the widest label, so every line of
             text starts at the same edge whatever the language. -->
        <ul v-if="openKeys.has(releaseKey(r))" class="grid grid-cols-[max-content_1fr] items-start gap-x-2.5 gap-y-1.5 px-4 pb-4 pl-10">
          <li v-for="(item, i) in r.items" :key="i" class="contents">
            <span class="mt-px rounded border border-border px-1.5 py-px text-center text-2xs text-muted-foreground">
              {{ t(`whatsNew.kind.${item.kind}`) }}
            </span>
            <span class="text-sm text-foreground leading-snug">{{ noteText(item, locale) }}</span>
          </li>
        </ul>
      </section>
    </Card>
  </div>
</template>
