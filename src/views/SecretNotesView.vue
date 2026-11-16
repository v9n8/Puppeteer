<script setup lang="ts">
// Secret notes as a screen of the main window (route `/secret-notes`; the user
// moved it here with "Move back to main window", lib/windowPlacement keeps the
// choice). The body is the one the own-window form uses
// (components/secretNotes/SecretNotesPane.vue). The main window's webview also
// renders Session text and other pages, so while the vault is open here the
// selection toolbar and Translate are off (composables/useVaultOpenGuard), and
// leaving the screen locks the vault.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { onBeforeRouteLeave, useRoute } from 'vue-router'
import { Lock, TextSelect } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import DetachButton from '@/components/app/DetachButton.vue'
import SecretNotesPane from '@/components/secretNotes/SecretNotesPane.vue'
import { secretNotesTarget } from '@/lib/secretNotesWindow'
import { vaultLock } from '@/lib/secretVault'

const { t } = useI18n()
const route = useRoute()
const pane = ref<InstanceType<typeof SecretNotesPane> | null>(null)

const first = (v: unknown): string | null => (typeof v === 'string' && v ? v : null)

// Asked again (sidebar, ⌘K, the menu) while the screen is open: re-point it.
watch(
  () => [route.query.project, route.query.id],
  ([project, id]) => pane.value?.applySelect({ projectId: first(project), id: first(id) }),
)

async function confirmLeave(): Promise<boolean> {
  return (await pane.value?.confirmLeave()) ?? true
}

// Leaving hides what is on screen and locks the vault; unsaved edits are asked about first.
onBeforeRouteLeave(async () => {
  if (!(await confirmLeave())) return false
  pane.value?.hideContent()
  await vaultLock().catch(() => {})
  return true
})
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <header class="flex h-12 shrink-0 items-center gap-3 border-b border-border/60 px-4">
      <h1 class="shrink-0 text-base font-semibold">{{ t('secretNotes.window.title') }}</h1>
      <p v-if="pane?.unlocked" class="flex min-w-0 flex-1 items-center gap-1.5 text-xs text-muted-foreground">
        <TextSelect class="h-4 w-4 shrink-0" :stroke-width="1.75" aria-hidden="true" />
        <span class="truncate">{{ t('secretNotes.toolbarOffNotice') }}</span>
      </p>
      <span v-else class="flex-1" />
      <Button v-if="pane?.unlocked" variant="outline" size="sm" :title="t('secretNotes.lockNow')" @click="pane?.lockNow()">
        <Lock class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
        <span class="max-[900px]:hidden">{{ t('secretNotes.lockNow') }}</span>
      </Button>
      <DetachButton :target="secretNotesTarget" :confirm-leave="confirmLeave" />
    </header>
    <SecretNotesPane
      ref="pane"
      mode="main"
      class="min-h-0 flex-1"
      :project-id="first(route.query.project)"
      :note-id="first(route.query.id)"
    />
  </div>
</template>
