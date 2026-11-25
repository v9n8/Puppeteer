// The ONE place a permission decision is written (remember the choice, then answer
// the backend). The global PermissionWindowManager (windows, session pop-outs) and
// the in-conversation block both call these, so there is no second write path.
import type { PermissionRequest } from '@/components/PermissionPrompt.vue'

/** The slice of the liveRuns store the decision needs (kept narrow so tests can fake it). */
export interface PermissionDecisionStore {
  get(runId: string): { permissionQueue: PermissionRequest[] } | undefined
  rememberAllowedTool(runId: string, tool: string): void
  rememberDeniedTool(runId: string, tool: string): void
  respondPermission(
    runId: string,
    req: PermissionRequest,
    decision: 'allow' | 'deny' | 'ask',
    opts?: { remember?: boolean; answers?: Record<string, string> },
  ): Promise<boolean>
}

// A decision for any request other than the head is stale (it was answered
// elsewhere) and must not be written.
function headIf(live: PermissionDecisionStore, runId: string, requestId: string) {
  const head = live.get(runId)?.permissionQueue[0]
  return head && head.request_id === requestId ? head : null
}

/** Allow / deny the head request. Resolves true only once the backend took the answer. */
export async function decidePermission(
  live: PermissionDecisionStore,
  runId: string,
  requestId: string,
  decision: 'allow' | 'deny',
  remember: boolean,
): Promise<boolean> {
  const req = headIf(live, runId, requestId)
  if (!req) return false
  // A once-only (broker) prompt has no standing rule: its `remember` asks the
  // backend to remember the command for this run only.
  if (remember && !req.once_only) {
    if (decision === 'allow') live.rememberAllowedTool(runId, req.tool_name)
    else live.rememberDeniedTool(runId, req.tool_name)
  }
  return live.respondPermission(runId, req, decision, { remember })
}

/** Answer the head AskUserQuestion request. */
export async function answerPermission(
  live: PermissionDecisionStore,
  runId: string,
  requestId: string,
  answers: Record<string, string>,
): Promise<boolean> {
  const req = headIf(live, runId, requestId)
  if (!req) return false
  return live.respondPermission(runId, req, 'allow', { answers })
}
