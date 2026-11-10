<script setup lang="ts">
// The composer under the AI result: palettes, attachments, prompt input and
// the run controls. RunView shows it once a run is open; the usage row that
// sat above the input is now RunStatusBar. State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import {
  Play, Square, FileText, Send, FolderClosed, Sparkles, ChevronDown, Maximize2, Minimize2, X,
  Paperclip, UserCircle, Navigation, Clock, Pencil, Quote,
} from 'lucide-vue-next'
import AppSelect from '@/components/AppSelect.vue'
import CapturePicker from '@/components/CapturePicker.vue'
import TodoChecklistPicker from '@/components/TodoChecklistPicker.vue'
import RunAttentionBanner from '@/components/RunAttentionBanner.vue'
import { Button } from '@/components/ui'
import RunComposerChips from './RunComposerChips.vue'
import { useRunSession } from './useRunSession'
import RunMcpSection from './RunMcpSection.vue'

const { t } = useI18n()
const {
  features, projectId, modelOverride, session, currentStatus, runSettingsOpen,
  runSettingsRoot, engineBadgeLabel, engineBadgeIcon, runSettingsTitle, toggleRunSettingsMenu,
  closeRunSettingsMenu, modelOptions, onModelSelect, composerModelLabel, followUpInput,
  sendingFollowUp, selectedClaudeAccountId, claudeAccountOptions, showClaudeAccountSelect, claudeAccountSelectDisabled,
  claudeAccountSelectTitle, handleClaudeAccountChange, pendingImages, pendingFiles, isDraggingFile,
  removePendingFile, onPickFiles, onComposerPaste, removePendingImage, chatEligible,
  chatPlaceholder, chatSendLabel, primaryDisabled, handlePrimaryAction,
  composerExpanded, toggleComposerExpand, composerEl, mentionOpen, mentionIndex, slashOpen,
  slashIndex, slashItems, selectSlash, onComposerInput, mentionItems, selectMention,
  insertCaptureRef, composerChecklist, applyChecklist, linkPicked,
  onComposerKeydown, handleSendFollowUp, handleCancel, queuedMessages, queueNotice, handleQueue,
  sendQueuedNow, removeQueuedMessage, editingQueuedId, queuedEditText, startQueuedEdit, saveQueuedEdit,
  cancelQueuedEdit, currentRunId, draftQuotes, removeDraftQuote, setDraftQuoteNote,
} = useRunSession()

function quoteFileLabel(q: { path: string; lines?: { start: number; end: number } }): string {
  const l = q.lines
  if (!l) return q.path
  return l.end > l.start ? `${q.path}:${l.start}-${l.end}` : `${q.path}:${l.start}`
}

// The queued-message editor takes focus as it opens.
const vFocus = { mounted: (el: HTMLTextAreaElement) => el.focus() }
</script>

<template>
  <div class="border-t border-border bg-card/40 px-3 py-2 shrink-0">
    <RunAttentionBanner
      v-if="currentStatus === 'running' && session?.attention.length"
      :notices="session.attention"
      @stop="handleCancel"
    />
    <div class="relative">
      <!-- Slash-command palette (anchored above the textarea) -->
      <div
        v-if="slashOpen && slashItems.length"
        class="absolute bottom-full left-0 mb-1 max-h-60 w-72 max-w-[80vw] overflow-y-auto rounded-md border border-border bg-popover shadow-lg z-20"
      >
        <ul class="py-1 text-xs">
          <li
            v-for="(cmd, i) in slashItems"
            :key="cmd"
            class="flex items-center gap-2 px-3 py-1.5 cursor-pointer font-mono"
            :class="i === slashIndex ? 'bg-primary/15 text-foreground' : 'text-foreground/80 hover:bg-muted'"
            @mousedown.prevent="selectSlash(cmd)"
            @mouseenter="slashIndex = i"
          >
            <span class="text-primary">/</span>
            <span class="truncate">{{ cmd }}</span>
          </li>
        </ul>
      </div>

      <!-- @-mention autocomplete popup (anchored above the textarea) -->
      <div
        v-if="mentionOpen && mentionItems.length"
        class="absolute bottom-full left-0 mb-1 max-h-60 w-[28rem] max-w-[80vw] overflow-y-auto rounded-md border border-border bg-popover shadow-lg z-20"
      >
        <ul class="py-1 text-xs">
          <li
            v-for="(item, i) in mentionItems"
            :key="item.path"
            class="flex items-center gap-2 px-3 py-1.5 cursor-pointer font-mono"
            :class="i === mentionIndex ? 'bg-primary/15 text-foreground' : 'text-foreground/80 hover:bg-muted'"
            @mousedown.prevent="selectMention(item)"
            @mouseenter="mentionIndex = i"
          >
            <component :is="item.is_dir ? FolderClosed : FileText" class="h-3.5 w-3.5 shrink-0 opacity-60" :stroke-width="2" />
            <span class="truncate">{{ item.path }}{{ item.is_dir ? '/' : '' }}</span>
          </li>
        </ul>
      </div>

      <!-- Messages queued while a turn runs (UX-25): sent one per turn end -->
      <div v-if="queuedMessages.length" class="mb-2 space-y-1.5">
        <p v-if="queueNotice" class="text-2xs text-muted-foreground">{{ queueNotice }}</p>
        <div class="flex flex-wrap gap-1.5">
          <div
            v-for="msg in queuedMessages"
            :key="msg.id"
            class="group flex items-center gap-1.5 rounded-md border border-primary/40 bg-primary/10 text-xs text-primary"
            :class="editingQueuedId === msg.id ? 'w-full items-start p-1.5' : 'h-7 max-w-72 pl-2 pr-1'"
            :title="editingQueuedId === msg.id ? undefined : msg.text"
          >
            <Clock class="h-3.5 w-3.5 shrink-0 opacity-70" :stroke-width="2" />
            <textarea
              v-if="editingQueuedId === msg.id"
              v-focus
              v-model="queuedEditText"
              rows="3"
              class="min-w-0 flex-1 resize-y bg-transparent font-mono text-xs text-foreground focus:outline-none"
              :aria-label="t('run.queuedEdit')"
              @keydown.enter.exact.prevent="saveQueuedEdit"
              @keydown.esc.prevent.stop="cancelQueuedEdit"
              @blur="saveQueuedEdit"
            />
            <template v-else>
              <span class="truncate">{{ msg.label || t('run.queuedAttachmentsOnly') }}</span>
              <span v-if="msg.attachments" class="flex shrink-0 items-center gap-0.5 opacity-70">
                <Paperclip class="h-3 w-3" :stroke-width="2" />{{ msg.attachments }}
              </span>
              <button
                class="h-5 w-5 shrink-0 rounded flex items-center justify-center hover:bg-background cursor-pointer"
                :title="t('run.queuedEdit')"
                :aria-label="t('run.queuedEdit')"
                @click="startQueuedEdit(msg.id)"
              >
                <Pencil class="h-3 w-3" :stroke-width="2" />
              </button>
              <button
                class="h-5 w-5 shrink-0 rounded flex items-center justify-center hover:bg-background cursor-pointer disabled:opacity-40 disabled:cursor-default"
                :disabled="sendingFollowUp"
                :title="t('run.queuedSendNow')"
                :aria-label="t('run.queuedSendNow')"
                @click="sendQueuedNow(msg.id)"
              >
                <Send class="h-3 w-3" :stroke-width="2" />
              </button>
              <button
                class="h-5 w-5 shrink-0 rounded flex items-center justify-center hover:bg-background cursor-pointer"
                :title="t('run.queuedRemove')"
                :aria-label="t('run.queuedRemove')"
                @click="removeQueuedMessage(msg.id)"
              >
                <X class="h-3 w-3" :stroke-width="2.5" />
              </button>
            </template>
          </div>
        </div>
      </div>

      <!-- Quote cards (UX-29): sent ahead of the message as blockquotes, each with an optional note -->
      <ol v-if="draftQuotes.length" class="mb-2 flex flex-col gap-1.5" :aria-label="t('run.quotes.title')">
        <li
          v-for="(q, qi) in draftQuotes"
          :key="q.id"
          class="group flex items-start gap-2 rounded-md border border-border bg-muted/40 px-2 py-1.5 text-xs"
        >
          <span class="mt-px flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full bg-primary/15 px-1 text-2xs font-semibold text-primary">{{ qi + 1 }}</span>
          <div class="min-w-0 flex-1">
            <p class="line-clamp-2 whitespace-pre-wrap text-foreground/80" :title="q.excerpt">
              <Quote class="mr-1 inline h-3 w-3 align-[-2px] opacity-50" :stroke-width="2" />{{ q.excerpt }}
            </p>
            <p v-if="q.kind === 'file'" class="mt-0.5 truncate font-mono text-2xs text-muted-foreground">{{ quoteFileLabel(q) }}</p>
            <input
              :value="q.note ?? ''"
              type="text"
              class="mt-1 w-full bg-transparent text-2xs text-foreground placeholder:text-muted-foreground/70 focus:outline-none"
              :placeholder="t('run.quotes.notePlaceholder')"
              :aria-label="t('run.quotes.notePlaceholder')"
              :disabled="sendingFollowUp"
              @input="setDraftQuoteNote(q.id, ($event.target as HTMLInputElement).value)"
            />
          </div>
          <button
            class="h-5 w-5 shrink-0 rounded flex items-center justify-center text-foreground/50 hover:text-foreground hover:bg-background cursor-pointer"
            :title="t('run.quotes.remove')"
            :aria-label="t('run.quotes.remove')"
            :disabled="sendingFollowUp"
            @click="removeDraftQuote(q.id)"
          >
            <X class="h-3 w-3" :stroke-width="2.5" />
          </button>
        </li>
      </ol>

      <!-- Pasted/attached image thumbnails -->
      <div v-if="pendingImages.length" class="flex flex-wrap gap-2 mb-2">
        <div
          v-for="img in pendingImages"
          :key="img.id"
          class="relative h-16 w-16 rounded-md border border-border overflow-hidden group"
        >
          <img :src="img.url" class="h-full w-full object-cover" alt="attachment" />
          <button
            class="absolute top-0.5 right-0.5 h-4 w-4 rounded-full bg-background/80 border border-border flex items-center justify-center text-foreground/70 hover:text-foreground hover:bg-background cursor-pointer opacity-0 group-hover:opacity-100 transition-opacity"
            :title="t('run.removeImage')"
            @click="removePendingImage(img.id)"
          >
            <X class="h-2.5 w-2.5" :stroke-width="2.5" />
          </button>
        </div>
      </div>

      <!-- Attached files referenced by path (drag-dropped non-images) -->
      <div v-if="pendingFiles.length" class="flex flex-wrap gap-1.5 mb-2">
        <div
          v-for="file in pendingFiles"
          :key="file.id"
          class="group flex items-center gap-1.5 h-7 pl-2 pr-1 rounded-md border border-border bg-muted/40 text-xs max-w-64"
          :title="file.path"
        >
          <FileText class="h-3.5 w-3.5 shrink-0 opacity-60" :stroke-width="2" />
          <span class="truncate font-mono">{{ file.name }}</span>
          <button
            class="h-4 w-4 rounded-full flex items-center justify-center text-foreground/50 hover:text-foreground hover:bg-background cursor-pointer shrink-0"
            :title="t('run.removeFile')"
            @click="removePendingFile(file.id)"
          >
            <X class="h-2.5 w-2.5" :stroke-width="2.5" />
          </button>
        </div>
      </div>

      <!-- Prompt input card: textarea on top, controls toolbar below -->
      <div class="relative rounded-lg border border-border bg-background focus-within:ring-1 focus-within:ring-ring transition-colors">
        <!-- Drag-and-drop hint: shown while an OS file is dragged over the window -->
        <div
          v-if="isDraggingFile"
          class="absolute inset-0 z-10 flex items-center justify-center gap-2 rounded-lg border-2 border-dashed border-primary bg-primary/10 text-xs font-medium text-primary pointer-events-none"
        >
          <Paperclip class="h-4 w-4" :stroke-width="2" />
          {{ t('run.dropFileToAttach') }}
        </div>
        <textarea
          ref="composerEl"
          v-model="followUpInput"
          rows="2"
          :placeholder="chatPlaceholder"
          class="w-full resize-none px-3 pt-2.5 pb-1 bg-transparent text-xs focus:outline-none font-mono min-h-[2.75rem] leading-relaxed"
          :disabled="sendingFollowUp"
          @keydown="onComposerKeydown"
          @input="onComposerInput"
          @click="onComposerInput"
          @compositionend="onComposerInput"
          @paste="onComposerPaste"
          @blur="mentionOpen = false; slashOpen = false"
        />

        <!-- Footer toolbar: attachments/actions plus compact run settings menu -->
        <div class="flex flex-wrap items-center gap-1.5 px-1.5 pb-1.5">
          <Button
            variant="ghost"
            size="icon"
            class="shrink-0"
            :disabled="sendingFollowUp"
            :title="t('run.attachFile')"
            :aria-label="t('run.attachFile')"
            @click="onPickFiles"
          >
            <Paperclip class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
          <CapturePicker
            :project-id="projectId"
            :disabled="sendingFollowUp"
            @insert="insertCaptureRef"
            @picked="linkPicked"
          />
          <TodoChecklistPicker
            v-if="composerChecklist"
            :todos="composerChecklist"
            :disabled="sendingFollowUp"
            @apply="applyChecklist"
          />
          <Button
            variant="ghost"
            size="icon"
            class="shrink-0"
            :title="composerExpanded ? t('run.collapseComposer') : t('run.expandComposer')"
            :aria-label="composerExpanded ? t('run.collapseComposer') : t('run.expandComposer')"
            @click="toggleComposerExpand"
          >
            <component :is="composerExpanded ? Minimize2 : Maximize2" class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
          <div
            ref="runSettingsRoot"
            class="relative inline-flex shrink-0"
            @keydown.esc.stop="closeRunSettingsMenu"
          >
            <Button
              variant="ghost"
              size="sm"
              class="px-2.5 text-foreground/70"
              :title="runSettingsTitle"
              :aria-label="runSettingsTitle"
              aria-haspopup="dialog"
              :aria-expanded="runSettingsOpen"
              @click.stop="toggleRunSettingsMenu"
            >
              <component :is="engineBadgeIcon" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
              <span class="text-xs font-medium shrink-0">{{ engineBadgeLabel }}</span>
              <span
                v-if="composerModelLabel"
                class="max-w-[9rem] truncate font-mono text-2xs text-muted-foreground"
                :title="composerModelLabel"
              >· {{ composerModelLabel }}</span>
              <ChevronDown
                class="h-3 w-3 shrink-0 text-muted-foreground transition-transform duration-200"
                :class="{ 'rotate-180': runSettingsOpen }"
                :stroke-width="2"
              />
            </Button>
            <transition
              enter-active-class="transition duration-150 ease-out"
              enter-from-class="opacity-0 translate-y-1"
              leave-active-class="transition duration-100 ease-in"
              leave-to-class="opacity-0 translate-y-1"
            >
              <div
                v-if="runSettingsOpen"
                role="dialog"
                class="absolute bottom-full left-1/2 z-30 mb-2 w-72 max-w-[calc(100vw-2rem)] -translate-x-1/2 rounded-md border border-border bg-popover p-3 shadow-lg shadow-black/20"
                :aria-label="t('run.runSettingsMenu')"
              >
                <div class="space-y-3">
                  <div class="space-y-1.5">
                    <div class="flex items-center gap-1.5 text-2xs font-medium uppercase text-muted-foreground">
                      <Sparkles class="h-3 w-3 shrink-0" :stroke-width="1.5" />
                      <span>{{ t('run.modelSetting') }}</span>
                    </div>
                    <AppSelect
                      :model-value="modelOverride"
                      @update:model-value="onModelSelect"
                      size="sm"
                      :options="modelOptions"
                      :disabled="currentStatus === 'running'"
                      class="w-full"
                      :title="t('run.modelTitle')"
                    >
                      <template #leading>
                        <Sparkles class="h-3 w-3 text-muted-foreground shrink-0" :stroke-width="1.5" />
                      </template>
                    </AppSelect>
                  </div>
                  <div v-if="showClaudeAccountSelect" class="space-y-1.5">
                    <div class="flex items-center gap-1.5 text-2xs font-medium uppercase text-muted-foreground">
                      <UserCircle class="h-3 w-3 shrink-0" :stroke-width="1.5" />
                      <span>{{ t('run.claudeAccountSetting') }}</span>
                    </div>
                    <AppSelect
                      :model-value="selectedClaudeAccountId"
                      @update:model-value="handleClaudeAccountChange"
                      size="sm"
                      :options="claudeAccountOptions"
                      :disabled="claudeAccountSelectDisabled"
                      class="w-full"
                      :title="claudeAccountSelectTitle"
                    >
                      <template #leading>
                        <UserCircle class="h-3 w-3 text-muted-foreground shrink-0" :stroke-width="1.5" />
                      </template>
                    </AppSelect>
                  </div>
                  <RunMcpSection
                    v-if="currentRunId"
                    :run-id="currentRunId"
                    :running="currentStatus === 'running'"
                  />
                </div>
              </div>
            </transition>
          </div>
          <RunComposerChips />

          <div class="ml-auto flex items-center gap-1.5 shrink-0">
            <Button
              v-if="currentStatus === 'running'"
              variant="destructive"
              class="shrink-0"
              :title="t('run.cancelRunningTurn')"
              @click="handleCancel"
            >
              <Square class="h-3.5 w-3.5" :stroke-width="2" fill="currentColor" />
              {{ t('run.cancel') }}
            </Button>
            <!-- While running, Cancel is the only action unless there is a
                 follow-up typed. Then the split action: Steer puts it into the
                 turn now (both engines take input mid-turn, D4-06 spike), Queue
                 (Enter) sends it once the turn ends. -->
            <div v-if="currentStatus === 'running' && !primaryDisabled" class="inline-flex shrink-0">
              <Button
                v-if="features.isOn('sessions.steer')"
                variant="outline"
                class="rounded-r-none border-r-0"
                :title="t('run.steerTitle')"
                @click="handleSendFollowUp(true)"
              >
                <Navigation class="h-3.5 w-3.5" :stroke-width="2" />
                {{ t('run.steer') }}
              </Button>
              <Button
                :class="{ 'rounded-l-none': features.isOn('sessions.steer') }"
                :title="t('run.queueTitle')"
                @click="handleQueue"
              >
                <Clock class="h-3.5 w-3.5" :stroke-width="2" />
                {{ t('run.queue') }}
              </Button>
            </div>
            <Button
              v-if="currentStatus !== 'running'"
              class="shrink-0"
              :disabled="primaryDisabled"
              @click="handlePrimaryAction"
              :title="currentStatus === 'fetched'
                ? t('run.startRunEnter')
                : chatEligible
                  ? t('run.resumeAndSend')
                  : t('run.rerunWithDefault')"
            >
              <component :is="chatSendLabel === t('run.sendLabelRun') ? Play : Send" class="h-3.5 w-3.5" :stroke-width="2" />
              {{ chatSendLabel }}
            </Button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
