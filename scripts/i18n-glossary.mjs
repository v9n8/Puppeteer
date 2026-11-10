/**
 * Glossary rule for the Vietnamese locale: some terms (Session, Project, Skill…)
 * stay in English, and a vi message that uses a Vietnamese translation of one
 * of them is reported. The term list lives in i18n-glossary.json next to this
 * file; check-i18n-messages.mjs runs the rule during `pnpm build:vite`.
 *
 * Kept free of file IO so the unit test can feed it any glossary
 * (src/lib/__tests__/i18nGlossary.test.mjs).
 */

// JS `\b` only knows ASCII letters, so "phiên" would "end" inside "phiên" itself
// (ê is not \w). A word edge is anything that is not a Unicode letter, combining
// mark, digit or underscore.
const WORD_CHAR = '[\\p{L}\\p{M}\\p{N}_]'

/** The mask written over a span once a phrase claims it; not a word char. */
const MASK = '\0'

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * Every phrase that applies to one message, longest first, with exceptions
 * ahead of banned words of the same length. Each phrase masks the span it
 * matches, so "phân nhánh" (Fork) is not reported again as "nhánh" (Branch),
 * and an exception such as "phiên bản" hides its "phiên" from the Session rule.
 */
function phrasesFor(glossary, key) {
  const list = []
  for (const { term, banned = [], ignore = [] } of glossary.terms) {
    for (const phrase of banned) list.push({ phrase, term })
    for (const phrase of ignore) list.push({ phrase, term: null })
  }
  for (const entry of glossary.allow ?? []) {
    if (entry.key === key) list.push({ phrase: entry.phrase, term: null })
  }
  return list.sort((a, b) => b.phrase.length - a.phrase.length || (a.term ? 1 : 0) - (b.term ? 1 : 0))
}

/**
 * The banned translations in one vi message, as `{ word, term }` where `word`
 * is the text as written (keeps its case) and `term` the English term to use.
 * `key` is `<namespace>.<dotted.key>`; it selects the `allow` entries.
 */
export function findBannedTerms(message, glossary, key) {
  let text = message.normalize('NFC')
  const hits = []
  for (const { phrase, term } of phrasesFor(glossary, key)) {
    const pattern = new RegExp(
      `(?<!${WORD_CHAR})${escapeRegExp(phrase.normalize('NFC'))}(?!${WORD_CHAR})`,
      'giu',
    )
    text = text.replace(pattern, (match) => {
      if (term) hits.push({ word: match, term })
      return MASK.repeat(match.length)
    })
  }
  return hits
}
