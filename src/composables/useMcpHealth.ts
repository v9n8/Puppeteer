import { computed, ref, watch, type Ref } from 'vue'
import { mcpToTest, type McpFailure, type McpHealth, type Source } from '@/lib/projectHealth'
import { isFormServer, useMcpServersStore, type FormMcpServer, type ProjectMcpServer } from '@/stores/mcpServers'

// Connection test of each MCP server a project has on (V3-48). A stdio test runs
// a real process, so servers are tested one at a time, once per project until
// the app closes; the answers live in memory only.
interface Entry {
  phase: 'loading' | 'checking' | 'idle' | 'done' | 'error'
  error: string
  enabled: number
  /** Names already tested, so coming back does not test them twice. */
  tested: string[]
  failures: McpFailure[]
}

const entries = ref<Record<string, Entry>>({})
const running = new Set<string>()

function testPayload(s: FormMcpServer<ProjectMcpServer>) {
  // No values: with the id, the backend uses the secrets already stored for the server.
  return {
    id: s.id,
    transport: s.transport,
    command: s.command,
    args: s.args,
    url: s.url,
    env: s.env_keys.map((key) => ({ key })),
    headers: s.header_keys.map((key) => ({ key })),
  }
}

async function check(projectId: string, isCurrent: () => boolean) {
  if (running.has(projectId) || entries.value[projectId]?.phase === 'done') return
  running.add(projectId)
  const store = useMcpServersStore()
  const prev = entries.value[projectId]
  const entry: Entry = prev
    ? { ...prev, phase: 'loading', error: '' }
    : { phase: 'loading', error: '', enabled: 0, tested: [], failures: [] }
  entries.value[projectId] = entry
  const live = () => entries.value[projectId]
  try {
    let list: ProjectMcpServer[]
    try {
      list = await store.listForProject(projectId)
    } catch (e) {
      Object.assign(live(), { phase: 'error', error: String(e) })
      return
    }
    const servers = mcpToTest(list)
    live().enabled = servers.length
    live().phase = 'checking'
    // A claude.ai connector has no command / url to test: it counts as on, untested.
    for (const s of servers.filter(isFormServer)) {
      if (live().tested.includes(s.name)) continue
      // The user left this project: stop here, the rest waits for the next visit.
      if (!isCurrent()) {
        live().phase = 'idle'
        return
      }
      let ok: boolean
      let message: string
      try {
        ({ ok, message } = await store.testConnection(testPayload(s)))
      } catch (e) {
        ok = false
        message = String(e)
      }
      live().tested.push(s.name)
      if (!ok) live().failures.push({ name: s.name, message, at: Date.now() })
    }
    live().phase = 'done'
  } finally {
    running.delete(projectId)
  }
}

/** Test result of the project, filled in as the tests finish. */
export function useMcpHealth(projectId: Ref<string>) {
  watch(projectId, (id) => { void check(id, () => projectId.value === id) }, { immediate: true })
  const mcp = computed<Source<McpHealth>>(() => {
    const e = entries.value[projectId.value]
    if (!e || e.phase === 'loading') return { state: 'loading' }
    if (e.phase === 'error') return { state: 'error', error: e.error }
    return {
      state: 'ready',
      value: { enabled: e.enabled, checking: e.phase !== 'done', failures: e.failures },
    }
  })
  return { mcp }
}
