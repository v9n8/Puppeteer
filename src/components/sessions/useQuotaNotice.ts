// Quota reached (V3-36): the app only tells which window is over and which
// other account is still usable; the user switches in the account chip. The
// facts come from the stored plan snapshots (lib/quotaNotice); this file adds
// the run's accounts and the sentences.
import { usageWindows } from '@/lib/usageRing'
import { quotaAlternatives, quotaNoticeText, type QuotaAccount, type QuotaNoticeParts } from '@/lib/quotaNotice'
import { useConfirm } from '@/composables/useConfirm'
import type { RunSessionCore } from './useRunSessionCore'
import type { useComposerChips } from './useComposerChips'

interface QuotaFacts {
  account: string
  parts: QuotaNoticeParts
}

export function useQuotaNotice(
  core: RunSessionCore,
  chips: ReturnType<typeof useComposerChips>,
  /** Usage row of the account the session runs `engine` on (the chip's account). */
  runUsageKey: (engine: string) => string,
) {
  const { t, currentRun, claudeStore, codexStore } = core
  const { notify } = useConfirm()

  // Only the run's own engine: the account chip refuses another one.
  async function quotaFacts(engine: string): Promise<QuotaFacts | null> {
    if (!currentRun.value || currentRun.value.engine !== engine) return null
    const accounts: QuotaAccount[] = engine === 'codex'
      ? [
          { value: '', label: t('run.accountMenu.sharedCodexLogin'), usageKey: 'codex' },
          ...codexStore.accounts.map((acc) => ({ value: acc.id, label: acc.label, usageKey: `codex:${acc.id}` })),
        ]
      : claudeStore.accounts.map((acc) => ({ value: acc.id, label: acc.label, usageKey: `claude:${acc.id}` }))
    const currentKey = runUsageKey(engine)
    await Promise.all([...new Set([currentKey, ...accounts.map((acc) => acc.usageKey)])].map(chips.loadUsage))
    const snapshots = chips.usageSnapshots.value
    const thresholds = chips.usageThresholds.value
    const now = Date.now()
    const best = quotaAlternatives(accounts, currentKey, snapshots, thresholds, now)[0]
    const parts = quotaNoticeText(
      usageWindows(snapshots[currentKey], now),
      best ? { account: best, windows: usageWindows(snapshots[best.usageKey], now) } : null,
      thresholds,
    )
    if (!parts) return null
    const account = accounts.find((acc) => acc.usageKey === currentKey)?.label
      ?? (engine === 'codex' ? t('run.accountMenu.sharedCodexLogin') : t('run.globalClaudeAccount'))
    return { account, parts }
  }

  function usedSentence({ account, parts }: QuotaFacts): string {
    return t('run.quotaNotice.used', { account, percent: parts.percent, window: windowName(parts) })
  }
  const windowName = (parts: QuotaNoticeParts) => t(parts.window === '5h' ? 'run.accountMenu.fiveHour' : 'run.accountMenu.week')

  function alternativeSentence({ account, parts }: QuotaFacts): string {
    if (!parts.other) return ''
    const key = parts.other.percent == null ? 'run.quotaNotice.alternativeNoFigure' : 'run.quotaNotice.alternative'
    return t(key, { other: parts.other.label, percent: parts.other.percent, account })
  }

  // Body of the "Budget exceeded" confirm; null when no window explains it.
  async function budgetOverMessage(engine: string): Promise<string | null> {
    const facts = await quotaFacts(engine)
    if (!facts) return null
    const { account, parts } = facts
    return t(parts.limit > 0 ? 'run.budgetOverMessage' : 'run.budgetOverMessageNoLimit', {
      account,
      percent: parts.percent,
      window: windowName(parts),
      limit: parts.limit,
      alternative: parts.other ? ` ${alternativeSentence(facts)}` : '',
    })
  }

  // After a turn tipped the account over: tell, with a single Close. Resolves
  // once closed; false when nothing was shown.
  async function showQuotaNotice(engine: string): Promise<boolean> {
    const facts = await quotaFacts(engine)
    if (!facts) return false
    await notify({
      title: t('run.quotaNotice.title', { account: facts.account }),
      message: [usedSentence(facts), alternativeSentence(facts)].filter(Boolean).join(' '),
      closeLabel: t('common.close'),
      variant: 'primary',
    })
    return true
  }

  return { budgetOverMessage, showQuotaNotice }
}
