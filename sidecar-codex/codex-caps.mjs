// What a codex app-server can do, by the version Rust read from `codex --version`
// (PUPPETEER_CODEX_VERSION): per method, true (has it), false (has not) or null
// (not known: tried, and the next one tried when the app-server refuses it).
//
// Sources (fixtures/*.methods.json, written by fixtures/extract-methods.mjs):
// - codex-cli 0.133.0-alpha.1, the oldest Puppeteer supports: its
//   `app-server generate-json-schema` has thread/rollback and no thread/revert.
// - openai/codex@d15bf1d (main, 2026-11): thread/rollback is removed
//   (codex-rs/app-server/README.md "Thread rollback") and
//   thread/revert { threadId, beforeTurnId } replaces it
//   (codex-rs/app-server-protocol/src/protocol/common.rs).
// Which release first has thread/revert, and which first drops thread/rollback,
// is not known (no release tag was checked), so both are null after 0.133.
// 0.0.x is a build from source, of any age: not known either.
export function codexCaps(version) {
  const m = /^(\d+)\.(\d+)\./.exec(version || '')
  const upTo133 = !!m && Number(m[1]) === 0 && Number(m[2]) >= 1 && Number(m[2]) <= 133
  return {
    'thread/rollback': upTo133 ? true : null,
    'thread/revert': upTo133 ? false : null,
  }
}

// The calls that drop a thread's last turns (Rewind, Fork), in the order to try
// them. thread/revert only takes a thread in codex's paginated history format
// (`historyMode: "paginated"` on the thread, which a codex without that format
// does not report), so such a thread tries it first and any other thread tries
// thread/rollback first.
export function rewindMethods(caps, historyMode) {
  const order = historyMode === 'paginated'
    ? ['thread/revert', 'thread/rollback']
    : ['thread/rollback', 'thread/revert']
  return order.filter((method) => caps[method] !== false)
}
