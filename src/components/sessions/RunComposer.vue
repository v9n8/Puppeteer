<script setup lang="ts">
// The composer under the AI result: palettes, attachments, usage row, prompt
// input and the run controls. RunView shows it once a run is open. State lives
// in useRunSession.
import { useI18n } from 'vue-i18n'
import {
  Play, Square, FileText, Send, FolderClosed, Sparkles, ChevronDown, Maximize2, Minimize2, X,
  Paperclip, ShieldQuestion, UserCircle, Navigation,
} from 'lucide-vue-next'
import AppSelect from '@/components/AppSelect.vue'
import CapturePicker from '@/components/CapturePicker.vue'
import SavedPromptPicker from '@/components/SavedPromptPicker.vue'
import TodoChecklistPicker from '@/components/TodoChecklistPicker.vue'
import ContextMeter from '@/components/ContextMeter.vue'
import BudgetBadge from '@/components/BudgetBadge.vue'
import RunAttentionBanner from '@/components/RunAttentionBanner.vue'
import { Button } from '@/components/ui'
import { PERMISSION_MODE_OPTIONS } from '@/lib/engineOptions'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  features, projectId, modelOverride, permissionMode, session, currentStatus, runSettingsOpen,
  runSettingsRoot, engineBadgeLabel, engineBadgeIcon, runSettingsTitle, toggleRunSettingsMenu,
  closeRunSettingsMenu, modelOptions, onModelSelect, contextTokens, contextModel,
  composerModelLabel, contextRateLimit, followUpInput, sendingFollowUp, selectedClaudeAccountId,
  claudeAccountOptions, showClaudeAccountSelect, claudeAccountSelectDisabled,
  claudeAccountSelectTitle, handleClaudeAccountChange, pendingImages, pendingFiles, isDraggingFile,
  removePendingFile, onPickFiles, onComposerPaste, removePendingImage, chatEligible,
  chatPlaceholder, chatSendLabel, primaryDisabled, handlePrimaryAction, sendSlashCommand,
  composerExpanded, toggleComposerExpand, composerEl, mentionOpen, mentionIndex, slashOpen,
  slashIndex, slashItems, selectSlash, onComposerInput, mentionItems, selectMention, savedPrompts,
  insertSavedPrompt, insertCaptureRef, composerChecklist, applyChecklist, linkPicked,
  onComposerKeydown, handleSendFollowUp, handleCancel,
} = useRunSession()
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

      <!-- Usage row: this run's context window on the left, the account
           plan-usage chips filling the width that used to sit empty.
           Both parts self-hide, so the row collapses when there is
           nothing to report. -->
      <div class="flex items-center gap-2 px-1 pb-1.5 min-w-0">
        <ContextMeter
          :tokens="contextTokens"
          :model="contextModel"
          :rate-limit="contextRateLimit"
          @compact="sendSlashCommand('/compact')"
        />
        <span v-if="contextTokens > 0 && features.isOn('usage.context_meter')" class="shrink-0 text-2xs text-muted-foreground/40">·</span>
        <!-- Unmounting it also stops its 60 s refresh timer. -->
        <BudgetBadge v-if="features.isOn('usage.context_meter')" />
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
          <SavedPromptPicker
            v-if="savedPrompts.length"
            :prompts="savedPrompts"
            :disabled="sendingFollowUp"
            @insert="insertSavedPrompt"
          />
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
                  <div class="space-y-1.5">
                    <div class="flex items-center gap-1.5 text-2xs font-medium uppercase text-muted-foreground">
                      <ShieldQuestion class="h-3 w-3 shrink-0" :stroke-width="1.5" />
                      <span>{{ t('run.permissionSetting') }}</span>
                    </div>
                    <AppSelect
                      v-model="permissionMode"
                      size="sm"
                      :options="PERMISSION_MODE_OPTIONS"
                      :disabled="currentStatus === 'running'"
                      class="w-full"
                      :title="t('run.permissionModeTitle')"
                    >
                      <template #leading>
                        <span class="text-2xs font-mono text-muted-foreground shrink-0">{{ t('run.perm') }}</span>
                      </template>
                    </AppSelect>
                  </div>
                </div>
              </div>
            </transition>
          </div>

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
            <!-- Steer: both engines take input mid-turn (D4-06 spike). -->
            <Button
              v-if="currentStatus === 'running' && !primaryDisabled && features.isOn('sessions.steer')"
              variant="outline"
              class="shrink-0"
              :title="t('run.steerTitle')"
              @click="handleSendFollowUp(true)"
            >
              <Navigation class="h-3.5 w-3.5" :stroke-width="2" />
              {{ t('run.steer') }}
            </Button>
            <!-- While running, Cancel is the only action unless there is a
                 follow-up typed — a disabled Send next to it is just noise. -->
            <Button
              v-if="currentStatus !== 'running' || !primaryDisabled"
              class="shrink-0"
              :disabled="primaryDisabled"
              @click="handlePrimaryAction"
              :title="currentStatus === 'running'
                ? t('run.sendMessageEnter')
                : currentStatus === 'fetched'
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
