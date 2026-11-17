<script setup lang="ts">
// "Links" button of the session info bar (V3-16): a link icon with the number of
// Slack threads, todos and notes tied to this session, and a menu with those
// three groups plus "New todo" / "New note" (the capture window filed under this
// session; a session made from an issue starts the todo from that issue). A group
// whose feature prop is off is neither listed nor counted; with all three off the
// button is not drawn. Rows open as the old header menus did.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Hash, Link2, Plus, Slack, StickyNote } from 'lucide-vue-next'
import { DropdownMenu, DropdownItem, DropdownSeparator } from '@/components/ui'
import TodoCaptureModal from '@/components/focus/TodoCaptureModal.vue'
import { openSlackWindow } from '@/lib/slackWindow'
import { openItemListWindow } from '@/lib/itemListWindow'
import { firstLine } from '@/lib/itemText'
import { linkSummary } from '@/lib/sessionInfoBar'
import type { RunLinkedItem } from '@/lib/sessionLinks'
import type { SlackThreadSummary } from '@/stores/slackThreads'
import { INFO_CHIP } from './infoBarStyle'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const { features, currentRun, linked, openQuickCapture, runGithubUrl } = useRunSession()

const enabled = computed(() => ({
  slack: features.allows('run.linkedSlack'),
  todos: features.allows('run.linkedTodos'),
  notes: features.allows('run.linkedNotes'),
}))

const summary = computed(() =>
  linkSummary(
    { slack: linked.threads.value.length, todos: linked.todos.value.length, notes: linked.notes.value.length },
    enabled.value,
  ),
)

const title = computed(() =>
  summary.value.total
    ? t('run.infoBar.links.title', {
        parts: summary.value.parts.map((p) => t(`run.infoBar.links.parts.${p.kind}`, p.count)).join(', '),
      })
    : t('run.infoBar.links.titleNone'),
)

function channelOf(th: SlackThreadSummary): string {
  return th.channel ?? th.channelId ?? ''
}

/** A note's title (else its first line); a todo's first line. */
function itemLabel(it: RunLinkedItem): string {
  return it.title.trim() || firstLine(it.text) || t('item.untitled')
}

function openItem(kind: 'todo' | 'note', id: string) {
  void openItemListWindow(kind, { id })
}

// A session made from an issue: "New todo" starts from the issue's title and link.
const issuePrefill = computed(() => {
  const run = currentRun.value
  const sourceUrl = run?.run_type === 'analyze_issue' ? runGithubUrl(run) : null
  if (!run || !sourceUrl) return null
  return { text: run.title ?? `#${run.ref_number}`, projectId: run.project_id, runId: run.id, sourceUrl }
})
const todoModalOpen = ref(false)

function newTodo() {
  if (issuePrefill.value && features.allows('todo.add')) todoModalOpen.value = true
  else openQuickCapture('todo')
}
</script>

<template>
  <DropdownMenu v-if="enabled.slack || enabled.todos || enabled.notes" align="right" prefer="above">
    <template #trigger="{ open }">
      <button type="button" :class="INFO_CHIP" :title="title" :aria-label="title" aria-haspopup="menu" :aria-expanded="open">
        <Link2 class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        <span v-if="summary.total" class="tabular-nums">{{ summary.total }}</span>
      </button>
    </template>
    <div class="w-[22rem]">
      <div class="max-h-[24rem] overflow-y-auto">
        <!-- Slack threads: only when there are some -->
        <template v-if="enabled.slack && linked.threads.value.length">
          <div class="px-2.5 pb-0.5 pt-1.5 text-2xs font-medium text-muted-foreground/70">
            {{ t('run.infoBar.links.slackGroup', { n: linked.threads.value.length }) }}
          </div>
          <DropdownItem
            v-for="th in linked.threads.value"
            :key="th.id"
            :title="t('sessionLink.openSlack')"
            @click="openSlackWindow({ id: th.id })"
          >
            <Slack class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <span class="min-w-0 flex-1 truncate">{{ th.title || t('slackThread.untitled') }}</span>
            <span v-if="channelOf(th)" class="flex shrink-0 items-center text-2xs text-muted-foreground">
              <Hash class="h-3 w-3" :stroke-width="2" />{{ channelOf(th) }}
            </span>
          </DropdownItem>
        </template>

        <!-- Todos: a tick box (flips the todo, keeps the menu open) + first line -->
        <template v-if="enabled.todos">
          <div class="px-2.5 pb-0.5 pt-1.5 text-2xs font-medium text-muted-foreground/70">
            {{ t('run.infoBar.links.todoGroup', { n: linked.todos.value.length }) }}
          </div>
          <p v-if="!linked.todos.value.length" class="px-2.5 py-1.5 text-2xs text-muted-foreground" @click.stop>
            {{ t('sessionLink.emptyTodos') }}
          </p>
          <DropdownItem
            v-for="td in linked.todos.value"
            :key="td.id"
            :title="t('sessionLink.openTodo')"
            @click="openItem('todo', td.id)"
          >
            <span
              role="checkbox"
              tabindex="0"
              :aria-checked="td.done"
              :aria-label="t('run.infoBar.links.toggleTodo')"
              :title="t('run.infoBar.links.toggleTodo')"
              class="-my-1.5 -ml-1.5 flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              @click.stop="linked.toggleTodo(td.id)"
              @keydown.enter.stop.prevent="linked.toggleTodo(td.id)"
              @keydown.space.stop.prevent="linked.toggleTodo(td.id)"
            >
              <span
                class="flex h-3.5 w-3.5 items-center justify-center rounded-[4px] border"
                :class="td.done ? 'border-primary bg-primary text-primary-foreground' : 'border-muted-foreground/40'"
              >
                <Check v-if="td.done" class="h-2 w-2" :stroke-width="3" />
              </span>
            </span>
            <span class="min-w-0 flex-1 truncate" :class="td.done && 'text-muted-foreground line-through'">
              {{ itemLabel(td) }}
            </span>
          </DropdownItem>
        </template>

        <!-- Notes -->
        <template v-if="enabled.notes">
          <div class="px-2.5 pb-0.5 pt-1.5 text-2xs font-medium text-muted-foreground/70">
            {{ t('run.infoBar.links.noteGroup', { n: linked.notes.value.length }) }}
          </div>
          <p v-if="!linked.notes.value.length" class="px-2.5 py-1.5 text-2xs text-muted-foreground" @click.stop>
            {{ t('sessionLink.emptyNotes') }}
          </p>
          <DropdownItem
            v-for="n in linked.notes.value"
            :key="n.id"
            :title="t('sessionLink.openNote')"
            @click="openItem('note', n.id)"
          >
            <StickyNote class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <span class="min-w-0 flex-1 truncate">{{ itemLabel(n) }}</span>
          </DropdownItem>
        </template>
      </div>

      <!-- Capture a new one, filed under this session (comes back linked). -->
      <template v-if="enabled.todos || enabled.notes">
        <DropdownSeparator />
        <DropdownItem v-if="enabled.todos" variant="primary" @click="newTodo">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('item.newTodo') }}
        </DropdownItem>
        <DropdownItem v-if="enabled.notes" variant="primary" @click="openQuickCapture('note')">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('item.newNote') }}
        </DropdownItem>
      </template>
    </div>
  </DropdownMenu>
  <TodoCaptureModal :open="todoModalOpen" :prefill="issuePrefill" @close="todoModalOpen = false" />
</template>
