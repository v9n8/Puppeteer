// Sub-agent ceiling (QO-06): a sub-agent never runs on a stronger model family
// than its session. The user picked the session's model to save quota; an
// `Agent` call that asks for more is lowered to the session's family before the
// CLI runs it. Only the ceiling: a floor would hang on the `[kind]` the agent
// writes itself, which is easy to get wrong.
//
// The broker turns this on with PUPPETEER_ROUTING_CEILING=1 (index.mjs), never
// for a side task (noTools). Each lowered call is reported as
//   { "type": "_puppeteer_routing_clamp", tool_use_id, from, to }
// for the app's routing record: the stream's `tool_use` still shows the input
// the agent asked for, not the one the hook put in its place.
//
// The hook only rewrites the input; it decides no permission, so a session that
// asks before a tool runs still asks.

const RANK = { haiku: 1, sonnet: 2, opus: 3 }

// The family of an alias or a Claude id, as the app reads it
// (`routing::tier::Model::family`): Fable counts as Opus. Unknown → undefined.
export function modelFamily(model) {
  const m = String(model ?? '').trim().toLowerCase()
  if (m.includes('fable') || m.includes('opus')) return 'opus'
  if (m.includes('sonnet')) return 'sonnet'
  if (m.includes('haiku')) return 'haiku'
  return undefined
}

// The alias to run instead of `requested`, or undefined when it is within the
// session's family. A side not known (no model asked, the session on the CLI's
// default) proves no ceiling, so nothing is lowered.
export function clampModel(requested, sessionModel) {
  const asked = modelFamily(requested)
  const ceiling = modelFamily(sessionModel)
  if (!asked || !ceiling || RANK[asked] <= RANK[ceiling]) return undefined
  return ceiling
}

// `hooks` plus the ceiling's `PreToolUse` hook on `Agent` (and `Task`, its older
// name). `send` writes one control message to the broker.
export function withRoutingCeiling(hooks, sessionModel, send) {
  const clamp = async (input) => {
    const toolInput = input.tool_input ?? {}
    const to = clampModel(toolInput.model, sessionModel)
    if (!to) return {}
    send({ type: '_puppeteer_routing_clamp', tool_use_id: input.tool_use_id, from: String(toolInput.model).trim(), to })
    return { hookSpecificOutput: { hookEventName: 'PreToolUse', updatedInput: { ...toolInput, model: to } } }
  }
  return { ...hooks, PreToolUse: [...(hooks?.PreToolUse ?? []), { matcher: 'Agent|Task', hooks: [clamp] }] }
}
