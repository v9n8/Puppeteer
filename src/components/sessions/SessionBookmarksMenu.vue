<script setup lang="ts">
// The header's Bookmarks button: the bookmarked turns of this session, each
// with its side (question / answer), label and when it was placed. Picking one
// jumps there like the prompt list does (a window read at its offset). Only
// shown once the session has a bookmark.
import { useI18n } from 'vue-i18n'
import { Bookmark, Sparkles, User } from 'lucide-vue-next'
import { Button, DropdownMenu, DropdownItem } from '@/components/ui'
import { bookmarkWhen } from '@/lib/bookmarkTime'
import { useRunSession } from './useRunSession'

const { t, locale } = useI18n()
const { placedBookmarks, jumpToSessionTurn, isViewingHistory, historyHasStream, liveHasStream } = useRunSession()

function whenLabel(createdAt: string): string {
  const when = bookmarkWhen(createdAt, new Date(), locale.value)
  if (!when) return ''
  return `${when.time} · ${when.day === 'date' ? when.date : t(`run.bookmarks.${when.day}`)}`
}
</script>

<template>
  <DropdownMenu
    v-if="placedBookmarks.length && (isViewingHistory ? historyHasStream : liveHasStream)"
    align="right"
  >
    <template #trigger>
      <Button
        variant="outline"
        size="icon-xs"
        :title="t('run.bookmarks.buttonTitle', { n: placedBookmarks.length })"
        :aria-label="t('run.bookmarks.buttonTitle', { n: placedBookmarks.length })"
      >
        <Bookmark class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
    </template>
    <div class="max-h-96 w-[26rem] max-w-[80vw] overflow-y-auto">
      <div class="px-2.5 pt-1 pb-1.5 text-2xs font-medium uppercase tracking-wide text-foreground/40">
        {{ t('run.bookmarks.count', { n: placedBookmarks.length }) }}
      </div>
      <DropdownItem
        v-for="placed in placedBookmarks"
        :key="placed.bookmark.id"
        :title="placed.bookmark.label || t('run.promptNoText')"
        @click="jumpToSessionTurn(placed.seq, true)"
      >
        <span class="flex h-5 min-w-5 shrink-0 items-center justify-center rounded bg-muted px-1 text-2xs font-mono tabular-nums text-foreground/45">{{ placed.seq + 1 }}</span>
        <span
          class="flex shrink-0"
          role="img"
          :title="t(`run.bookmarks.${placed.bookmark.side}`)"
          :aria-label="t(`run.bookmarks.${placed.bookmark.side}`)"
        >
          <component
            :is="placed.bookmark.side === 'question' ? User : Sparkles"
            class="h-3.5 w-3.5 text-foreground/50"
            :stroke-width="1.75"
          />
        </span>
        <span
          class="min-w-0 flex-1 truncate"
          :class="placed.bookmark.label ? '' : 'italic text-foreground/40'"
        >{{ placed.bookmark.label || t('run.promptNoText') }}</span>
        <span class="shrink-0 text-2xs tabular-nums text-foreground/45">{{ whenLabel(placed.bookmark.created_at) }}</span>
      </DropdownItem>
    </div>
  </DropdownMenu>
</template>
