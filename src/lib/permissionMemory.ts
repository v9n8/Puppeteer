// The project's standing "Always" allow/deny choices for permission prompts
// (components/PermissionPrompt.vue offers them, stores/liveRuns.ts applies
// them). Kept free of Vue / Tauri / `@/` imports so it can be unit-tested with
// `node --test`.

/** The fields of a permission request these rules read. */
export interface RememberableRequest {
  tool_name: string
  once_only?: boolean
}

/**
 * Whether the prompt may offer "Always" (allow or deny). A plan review is not
 * a tool to remember. Bare `MCP` is the fallback an engine emits when it can't
 * name the server/tool, so remembering it would silently allow every MCP
 * request from every server. Keyed prompts (`mcp__<server>[__<tool>]`) are
 * rememberable. A broker prompt (`once_only`: gh / aws / terraform …) is keyed
 * on the CLI alone, so "Always" for `terraform apply` would also pass a later
 * `terraform destroy` — it is answered once.
 */
export function canRememberPermission(req: RememberableRequest): boolean {
  return req.tool_name !== 'ExitPlanMode' && req.tool_name !== 'MCP' && !req.once_only
}

/**
 * The answer a standing choice gives without asking, or `null` to show the
 * prompt. AskUserQuestion must always reach the user — auto-deciding it would
 * submit empty answers. Deny wins if both somehow apply. A `once_only` prompt
 * skips the allow list (an older Always, or one saved for a non-broker call of
 * the same name, must not pass it) but still honors the deny list — refusing
 * without asking is safe.
 */
export function standingPermissionDecision(
  req: RememberableRequest,
  allowedTools: readonly string[],
  deniedTools: readonly string[],
): 'allow' | 'deny' | null {
  if (req.tool_name === 'AskUserQuestion') return null
  if (deniedTools.includes(req.tool_name)) return 'deny'
  if (!req.once_only && allowedTools.includes(req.tool_name)) return 'allow'
  return null
}
