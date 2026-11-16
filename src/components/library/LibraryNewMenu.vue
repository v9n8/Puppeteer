<script setup lang="ts">
// "New" in the Library's list column (V3-50): pick a kind, name it, and the
// backend writes a template file where the engine reads it (library_create).
// Kinds with no folder in this scope are not offered. The host rescans and opens
// the new item; local-only, so the host does not mount this in a remote window.
import { computed, nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus } from 'lucide-vue-next'
import { Button, DropdownItem, DropdownMenu, Input, Modal } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import {
  CMD_LIBRARY_CREATE, createTargets, isLibraryExists, validItemName,
  type CreateTarget, type DiskItem, type LibraryScope,
} from '@/lib/libraryDisk'

const props = defineProps<{
  scope: LibraryScope
  projectId: string | null
  /** "Global", or the project's name: the menu says where the item goes. */
  place: string
  /** The project's default engine; Global starts on Claude. */
  defaultEngine?: string | null
  disabled?: boolean
}>()
const emit = defineEmits<{ created: [item: DiskItem] }>()

const { t } = useI18n()

const targets = computed(() => createTargets(props.scope, props.defaultEngine))

const target = ref<CreateTarget | null>(null)
const engine = ref<'claude' | 'codex'>('claude')
const name = ref('')
const creating = ref(false)
const serverError = ref<string | null>(null)
const inputEl = ref<InstanceType<typeof Input> | null>(null)

const kindLabel = computed(() => (target.value ? t(`library.kindOne.${target.value.kind}`) : ''))
// Shown only once something is typed, so an empty box is not an error yet.
const nameError = computed(() =>
  name.value !== '' && !validItemName(name.value) ? t('library.newNameInvalid') : (serverError.value ?? ''),
)
const canCreate = computed(() => validItemName(name.value) && !creating.value)

async function pick(next: CreateTarget) {
  target.value = next
  engine.value = next.engine
  name.value = ''
  serverError.value = null
  await nextTick()
  ;((inputEl.value?.$el ?? null) as HTMLInputElement | null)?.focus()
}

function close() {
  if (!creating.value) target.value = null
}

async function create() {
  const picked = target.value
  if (!picked || !canCreate.value) return
  creating.value = true
  serverError.value = null
  try {
    const created = await invoke<DiskItem>(CMD_LIBRARY_CREATE, {
      scope: props.scope,
      projectId: props.projectId,
      kind: picked.kind,
      engine: engine.value,
      name: name.value.trim(),
    })
    target.value = null
    emit('created', created)
  } catch (e) {
    serverError.value = isLibraryExists(e)
      ? t('library.newExists', { kind: t(`library.kindOne.${picked.kind}`) })
      : String(e)
  } finally {
    creating.value = false
  }
}
</script>

<template>
  <DropdownMenu align="left">
    <template #trigger>
      <Button size="sm" class="shrink-0" :disabled="disabled || targets.length === 0">
        <Plus class="h-3.5 w-3.5" :stroke-width="2" />
        {{ t('library.new') }}
      </Button>
    </template>
    <p class="px-2.5 py-1.5 text-xs text-muted-foreground">{{ t('library.newIn', { place }) }}</p>
    <DropdownItem v-for="item in targets" :key="item.kind" class="min-h-7" @click="pick(item)">
      {{ t(`library.kindOne.${item.kind}`) }}
    </DropdownItem>
  </DropdownMenu>

  <Modal :open="!!target" :title="t('library.newTitle', { kind: kindLabel })" size="sm" @close="close">
    <div class="space-y-3 px-4 py-4" @keydown.esc.prevent="close">
      <div v-if="target && target.engines.length > 1" class="space-y-1.5">
        <span class="block text-xs font-medium text-muted-foreground">{{ t('library.newEngine') }}</span>
        <div class="flex gap-2" role="radiogroup" :aria-label="t('library.newEngine')">
          <Button
            v-for="e in target.engines"
            :key="e"
            size="sm"
            :variant="engine === e ? 'primary' : 'outline'"
            role="radio"
            :aria-checked="engine === e"
            @click="engine = e"
          >
            {{ t(`library.engine.${e}`) }}
          </Button>
        </div>
      </div>
      <div class="space-y-1.5">
        <label class="block text-xs font-medium text-muted-foreground" for="library-new-name">{{ t('library.newName') }}</label>
        <Input
          id="library-new-name"
          ref="inputEl"
          v-model="name"
          class="font-mono"
          @update:model-value="serverError = null"
          @keydown.enter.prevent="create"
        />
        <p v-if="nameError" class="text-xs text-destructive" role="alert">{{ nameError }}</p>
      </div>
    </div>
    <template #footer>
      <Button variant="outline" :disabled="creating" @click="close">{{ t('common.cancel') }}</Button>
      <Button variant="primary" :disabled="!canCreate" @click="create">{{ t('library.newCreate') }}</Button>
    </template>
  </Modal>
</template>
