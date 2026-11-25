//! Loopback Streamable-HTTP MCP server exposing the `session_*` control tools to
//! conductor runs. Hand-rolled JSON-RPC 2.0 over a single `POST /mcp` endpoint,
//! served by axum on 127.0.0.1 only. Every request must carry the conductor's
//! bearer token; tools are scoped to workers that conductor spawned.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

use super::{commands, tools, ConductorState, FEATURE};
use crate::db::Db;
use crate::features;

const PROTOCOL_VERSION: &str = "2025-06-18";

/// How often the streaming `session_wait` emits a `notifications/progress` while a
/// worker is still running. Each progress message keeps the client's tool-call
/// alive (per MCP, progress resets the call timeout) and carries live status so
/// the conductor — and the UI — can track the wait instead of it silently timing
/// out. Must be comfortably below any client tool timeout.
const PROGRESS_INTERVAL_MS: u64 = 15_000;

#[derive(Clone)]
pub struct HttpState {
    pub app: AppHandle,
    pub conductor: ConductorState,
}

/// Bring the loopback MCP server in line with the [`FEATURE`] switch: up while
/// it is on; once it is off, down as soon as no conductor or worker is still
/// running (a conductor running when the switch went off keeps its tools to
/// the end, see `commands::ensure_conductor_launch`). Called at startup, by
/// the `features::apply` hook and after every finished run.
pub async fn follow(app: &AppHandle, conductor: &ConductorState) {
    let db = app.state::<Db>().inner().clone();
    let mut slot = conductor.server.lock().await;
    let on = features::is_on(&db, FEATURE).await;
    let in_flight = !on && commands::any_running(&db).await;
    let state = HttpState {
        app: app.clone(),
        conductor: conductor.clone(),
    };
    let started = follow_locked(&mut slot, conductor, on, in_flight, || router(state)).await;
    drop(slot);
    if started {
        // Restore ownership of conductor sessions still `running` from before
        // an app restart (or the switch going off), so their resume keeps control.
        commands::reconnect_running(&db, conductor).await;
        install_wake_consumer(app.clone(), conductor);
    }
}

/// Start (`on`), keep (`in_flight`) or stop the server. Returns whether it was
/// started now. Caller holds the server lock.
async fn follow_locked(
    slot: &mut Option<tokio::task::JoinHandle<()>>,
    conductor: &ConductorState,
    on: bool,
    in_flight: bool,
    make_router: impl FnOnce() -> Router,
) -> bool {
    if on {
        return start_locked(slot, conductor, make_router).await;
    }
    if !in_flight {
        stop_locked(slot, conductor).await;
    }
    false
}

/// Bind an ephemeral 127.0.0.1 port, record it in `ConductorState` and serve.
/// No-op when already running (a task that exited counts as stopped).
async fn start_locked(
    slot: &mut Option<tokio::task::JoinHandle<()>>,
    conductor: &ConductorState,
    make_router: impl FnOnce() -> Router,
) -> bool {
    if slot.as_ref().is_some_and(|t| !t.is_finished()) {
        return false;
    }
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", 0)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(event = "conductor_mcp_start_failed", error = %e);
            return false;
        }
    };
    let Ok(port) = listener.local_addr().map(|a| a.port()) else {
        return false;
    };
    conductor.set_port(port);
    tracing::info!(event = "conductor_mcp_listening", port = port);
    let router = make_router();
    *slot = Some(tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(event = "conductor_mcp_serve_failed", error = %e);
        }
    }));
    true
}

/// Close the port. Only called with nothing left running, so no conductor is
/// mid-call. No-op when not running.
async fn stop_locked(slot: &mut Option<tokio::task::JoinHandle<()>>, conductor: &ConductorState) {
    let Some(task) = slot.take() else {
        return;
    };
    task.abort();
    // Wait for the task to drop its listener, so the port is free on return.
    let _ = task.await;
    conductor.set_port(0);
    tracing::info!(event = "conductor_mcp_stopped");
}

fn router(state: HttpState) -> Router {
    Router::new()
        .route("/mcp", post(handle_post).get(handle_get).delete(handle_delete))
        .with_state(state)
}

/// Wake consumer: the sidecar drain pushes every finished run's id here (a plain
/// channel send); this task performs the resume/slot-release off the drain's
/// call stack, breaking the drain → resume_run → drain type cycle. A restarted
/// server installs a new one; the old task ends when its sender is replaced.
fn install_wake_consumer(app: AppHandle, conductor: &ConductorState) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<super::WakeEvent>();
    conductor.set_wake_sender(tx);
    let conductor = conductor.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                super::WakeEvent::Exited(run_id) => super::wake::on_run_finished(&app, &run_id).await,
                super::WakeEvent::TurnEnded(run_id) => super::wake::on_worker_turn_ended(&app, &run_id).await,
            }
            // With the switch off, the last run to finish closes the port.
            follow(&app, &conductor).await;
        }
    });
}

/// Streamable HTTP: the client opens a GET for a server->client SSE stream. We
/// don't push server-initiated messages, so decline it; the SDK tolerates 405.
async fn handle_get() -> Response {
    StatusCode::METHOD_NOT_ALLOWED.into_response()
}

/// Session teardown (DELETE) — we keep no HTTP session state, so just ack.
async fn handle_delete() -> Response {
    StatusCode::OK.into_response()
}

async fn handle_post(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    // ---- auth: bearer token -> conductor run id -----------------------------
    let token = bearer(&headers);
    let conductor_run_id = match token.and_then(|t| state.conductor.resolve_token(&t)) {
        Some(id) => id,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid or missing session token" })),
            )
                .into_response()
        }
    };

    // A JSON-RPC notification (no `id`) gets no response body.
    let id = body.get("id").cloned();
    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = body.get("params").cloned().unwrap_or(Value::Null);

    if id.is_none() {
        // notifications/initialized, notifications/cancelled, etc.
        return StatusCode::ACCEPTED.into_response();
    }
    let id = id.unwrap();

    // `session_wait` can block for minutes. Serve it as an SSE stream that emits
    // periodic progress (keeping the client's tool call alive) and finally the
    // JSON-RPC result — instead of a single response the client aborts with "The
    // operation timed out." before it ever sees the real `waited_ms`.
    if method == "tools/call"
        && params.get("name").and_then(|n| n.as_str()) == Some("session_wait")
    {
        return session_wait_stream(&state, &conductor_run_id, id, &params).await;
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "conductor", "version": "0.1.0" },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_specs() })),
        "tools/call" => {
            handle_tools_call(&state, &conductor_run_id, &params).await
        }
        other => Err(rpc_error(-32601, &format!("method not found: {}", other))),
    };

    let payload = match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err(err) => json!({ "jsonrpc": "2.0", "id": id, "error": err }),
    };
    (StatusCode::OK, Json(payload)).into_response()
}

/// Dispatch a `tools/call` to the matching handler, wrapping the structured
/// result in MCP `content` blocks. Tool-level failures come back as
/// `{ isError: true }` (per MCP) rather than a JSON-RPC error, so the model can
/// read and react to them.
async fn handle_tools_call(
    state: &HttpState,
    conductor_run_id: &str,
    params: &Value,
) -> Result<Value, Value> {
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| rpc_error(-32602, "missing tool name"))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let outcome = tools::dispatch(state, conductor_run_id, name, args).await;
    Ok(match outcome {
        Ok(value) => tool_result_ok(value),
        Err(msg) => tool_result_err(&msg),
    })
}

/// Wrap a successful tool payload in MCP `content` blocks.
fn tool_result_ok(value: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": value.to_string() }],
        "isError": false,
    })
}

/// Wrap a tool-level failure as an MCP `{ isError: true }` result (not a JSON-RPC
/// error) so the model can read and react to it.
fn tool_result_err(msg: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": json!({ "error": msg }).to_string() }],
        "isError": true,
    })
}

/// Stream a `session_wait` call as Server-Sent Events: emit `notifications/progress`
/// every `PROGRESS_INTERVAL_MS` while workers run, then the final JSON-RPC result,
/// then close. Keeps the client's tool call alive through long waits and delivers
/// the real `waited_ms`/`waited_human` instead of a client-side timeout.
async fn session_wait_stream(
    state: &HttpState,
    conductor_run_id: &str,
    id: Value,
    params: &Value,
) -> Response {
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    // The client's progress token (if any) must be echoed on every progress
    // notification so the client can correlate it with this call.
    let progress_token = params
        .get("_meta")
        .and_then(|m| m.get("progressToken"))
        .cloned();

    // Resolve ids up front so a bad request fails fast as a one-shot result.
    let ids = match tools::resolve_wait_ids(state, conductor_run_id, &args).await {
        Ok(ids) => ids,
        Err(msg) => {
            let payload = json!({ "jsonrpc": "2.0", "id": id, "result": tool_result_err(&msg) });
            return one_shot_sse(payload);
        }
    };
    let return_on = match super::wait::return_on(&args) {
        Ok(r) => r,
        Err(msg) => {
            let payload = json!({ "jsonrpc": "2.0", "id": id, "result": tool_result_err(&msg) });
            return one_shot_sse(payload);
        }
    };
    let timeout_ms = tools::wait_timeout_ms(&args);

    struct WaitState {
        state: HttpState,
        ids: Vec<String>,
        watch: super::wait::Watch,
        id: Value,
        progress_token: Option<Value>,
        timeout_ms: u64,
        started: Instant,
        deadline: Instant,
        done: bool,
    }

    let started = Instant::now();
    let init = WaitState {
        state: state.clone(),
        ids,
        watch: super::wait::Watch::new(return_on),
        id,
        progress_token,
        timeout_ms,
        started,
        deadline: started + Duration::from_millis(timeout_ms),
        done: false,
    };

    let stream = futures_util::stream::unfold(init, |mut st| async move {
        if st.done {
            return None;
        }
        let tick_start = Instant::now();
        loop {
            let (done, pending) = match tools::wait_snapshot(&st.state, &st.ids).await {
                Ok(snap) => snap,
                Err(msg) => {
                    st.done = true;
                    let payload =
                        json!({ "jsonrpc": "2.0", "id": st.id, "result": tool_result_err(&msg) });
                    let ev = Event::default().data(payload.to_string());
                    return Some((Ok::<_, Infallible>(ev), st));
                }
            };
            let now = Instant::now();
            // Terminal: `return_on` satisfied, or the wait ceiling reached.
            if let Some(end) = st.watch.check(&pending, now >= st.deadline) {
                st.done = true;
                let waited_ms = st.started.elapsed().as_millis() as u64;
                let timed_out = end == super::wait::End::TimedOut;
                let result = tools::build_wait_result(done, pending, waited_ms, timed_out);
                let payload =
                    json!({ "jsonrpc": "2.0", "id": st.id, "result": tool_result_ok(result) });
                let ev = Event::default().data(payload.to_string());
                return Some((Ok(ev), st));
            }
            // Time to heartbeat: emit a progress notification and yield so the
            // client keeps the call alive.
            if now.duration_since(tick_start) >= Duration::from_millis(PROGRESS_INTERVAL_MS) {
                let waited_ms = st.started.elapsed().as_millis() as u64;
                let mut progress = json!({
                    "progress": waited_ms,
                    "total": st.timeout_ms,
                    "message": format!(
                        "{} worker(s) still running — {} elapsed",
                        pending.len(),
                        tools::format_waited(waited_ms)
                    ),
                });
                if let Some(tok) = &st.progress_token {
                    progress["progressToken"] = tok.clone();
                }
                let payload = json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": progress,
                });
                let ev = Event::default().data(payload.to_string());
                return Some((Ok(ev), st));
            }
            tokio::time::sleep(Duration::from_millis(tools::POLL_INTERVAL_MS)).await;
        }
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// A single JSON-RPC message delivered as a one-event SSE stream (used for
/// fail-fast errors on the streaming endpoint).
fn one_shot_sse(payload: Value) -> Response {
    let stream = futures_util::stream::once(async move {
        Ok::<_, Infallible>(Event::default().data(payload.to_string()))
    });
    Sse::new(stream).into_response()
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
}

fn rpc_error(code: i64, message: &str) -> Value {
    json!({ "code": code, "message": message })
}

/// Static tool schema advertised via `tools/list`.
fn tool_specs() -> Value {
    json!([
        {
            "name": "session_spawn",
            "description": "Spawn a new worker session to do a piece of the work. Returns { worker_id }.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "role_label": { "type": "string", "description": "Short label for this worker's role, e.g. 'implementer'." },
                    "prompt": { "type": "string", "description": "The worker's first task message." },
                    "instruction": { "type": "string", "description": "Optional role/system framing appended to the worker's system prompt." },
                    "engine": { "type": "string", "enum": ["claude", "codex"], "description": "Engine for the worker. Defaults to project default." },
                    "model": { "type": "string", "description": "Optional model override. Must belong to the worker's engine; a mismatched id is ignored and the worker uses that engine's global default model. Omit to use the global default." },
                    "permission_mode": { "type": "string", "enum": ["default", "acceptEdits", "plan"], "description": "Worker permission mode; never looser than your own mode (a looser one is refused). Omit it to give the worker your own mode (or the nearest stricter one its engine has); the human answers any prompts in the permission drawer." },
                    "effort": { "type": "string", "enum": super::defaults::SPAWN_EFFORT_LEVELS, "description": "Optional reasoning effort for the worker. Set it only when your instructions tell you how to pick one; omit it to use the user's worker default." },
                    "task_kind": { "type": "string", "enum": super::defaults::TASK_KINDS, "description": "Optional kind of work this worker does, for the user's usage stats." }
                },
                "required": ["prompt"]
            }
        },
        {
            "name": "session_list",
            "description": "List all workers you have spawned and their current status.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "session_poll",
            "description": "Instant status of the given workers (or all yours if omitted).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "worker_ids": { "type": "array", "items": { "type": "string" } }
                }
            }
        },
        {
            "name": "session_wait",
            "description": "Block until one of the given workers that is running now finishes its turn (return_on \"any\", the default) or all of them are idle (\"all\"), or timeout_ms elapses. A worker already idle when you call is listed in `done` but does not end the wait; with none running the call returns at once. `done` lists every given worker idle at return, `pending` those still running. Streams progress while it waits (kept alive, not aborted) and always returns real data. The result's `waited_human` (e.g. \"1m 3s\") and `waited_ms` are how long THIS call actually blocked — quote `waited_human` verbatim as the wait time. NEVER report timeout_ms (the ceiling you asked for); doing so is what turns a 1-minute wait into a false \"10 minutes\". `timed_out: true` means the ceiling passed before that happened: the workers are still running. Prefer ending your turn while workers run (the auto-resume wakes you within seconds of one finishing); use this only when a worker is about to finish, with a short timeout_ms (at most 120000).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "worker_ids": { "type": "array", "items": { "type": "string" } },
                    "timeout_ms": { "type": "integer", "description": "Max wait in ms (default 120000, capped at 600000). Keep it short; end your turn for longer work." },
                    "return_on": { "type": "string", "enum": ["any", "all"], "description": "\"any\" (default): return when one worker running at call time finishes. \"all\": return only when every worker is idle." }
                },
                "required": ["worker_ids"]
            }
        },
        {
            "name": "session_read",
            "description": "Read a worker's latest assistant reply (its result for the last turn). A long reply comes in pages: on `truncated: true`, call again with `offset` = `next_offset`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "worker_id": { "type": "string" },
                    "max_chars": { "type": "integer", "description": "Page size in chars (default 8000, max 40000)." },
                    "offset": { "type": "integer", "description": "Char to start from (default 0)." }
                },
                "required": ["worker_id"]
            }
        },
        {
            "name": "session_send",
            "description": "Give a worker a follow-up turn with new instructions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "worker_id": { "type": "string" },
                    "text": { "type": "string" }
                },
                "required": ["worker_id", "text"]
            }
        },
        {
            "name": "session_cancel",
            "description": "Cancel a worker's currently running turn.",
            "inputSchema": {
                "type": "object",
                "properties": { "worker_id": { "type": "string" } },
                "required": ["worker_id"]
            }
        }
    ])
}

/// Names declared in `specs` that `prompt` never mentions: the conductor only
/// uses tools its system prompt teaches, so a new tool must be named there.
/// Matched as `name(` (how the prompt lists them) so `session_read` is not
/// satisfied by a longer name such as `session_read_all`.
#[cfg(test)]
fn tools_missing_from(prompt: &str, specs: &Value) -> Vec<String> {
    specs
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|spec| spec["name"].as_str())
        .filter(|name| !prompt.contains(&format!("{name}(")))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conductor::CONDUCTOR_SYSTEM_PROMPT;

    #[test]
    fn every_tool_is_in_the_conductor_prompt() {
        let specs = tool_specs();
        // An empty or reshaped list would make the check below pass vacuously.
        assert!(specs.as_array().is_some_and(|s| !s.is_empty()));
        let missing = tools_missing_from(CONDUCTOR_SYSTEM_PROMPT, &specs);
        assert!(missing.is_empty(), "CONDUCTOR_SYSTEM_PROMPT never names these tools: {missing:?}");
    }

    #[test]
    fn spawn_takes_an_optional_effort_and_task_kind() {
        let specs = tool_specs();
        let spawn = &specs.as_array().unwrap()[0];
        assert_eq!(spawn["name"], "session_spawn");
        let props = &spawn["inputSchema"]["properties"];
        assert_eq!(props["effort"]["enum"], json!(["low", "medium", "high"]));
        assert_eq!(
            props["task_kind"]["enum"],
            json!(["search", "plan", "implement", "review", "test", "docs", "other"])
        );
        assert_eq!(spawn["inputSchema"]["required"], json!(["prompt"]));
    }

    #[test]
    fn missing_tool_is_reported() {
        let mut specs = tool_specs();
        specs.as_array_mut().unwrap().push(json!({ "name": "session_x" }));
        assert_eq!(tools_missing_from(CONDUCTOR_SYSTEM_PROMPT, &specs), ["session_x"]);
    }

    async fn accepts(port: u16) -> bool {
        tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok()
    }

    #[tokio::test]
    async fn server_follows_the_switch_and_waits_for_running_conductors() {
        let cst = ConductorState::new();
        let mut slot = None;

        // Switch off with nothing running (fresh install turned off): no port.
        assert!(!follow_locked(&mut slot, &cst, false, false, Router::new).await);
        assert!(slot.is_none());
        assert_eq!(cst.port(), 0);

        // On: bound once; following again keeps the same server.
        assert!(follow_locked(&mut slot, &cst, true, false, Router::new).await);
        let port = cst.port();
        assert_ne!(port, 0);
        assert!(accepts(port).await);
        assert!(!follow_locked(&mut slot, &cst, true, false, Router::new).await);
        assert_eq!(cst.port(), port);

        // Off while a conductor still runs: it keeps its server.
        assert!(!follow_locked(&mut slot, &cst, false, true, Router::new).await);
        assert_eq!(cst.port(), port);
        assert!(accepts(port).await);

        // The last one finished: the port is closed and can be bound again. Not
        // probed with `connect`: on macOS under load a refused loopback connect
        // can come back `Ok` with a dead socket (same as the Inbox API test).
        assert!(!follow_locked(&mut slot, &cst, false, false, Router::new).await);
        assert!(slot.is_none());
        assert_eq!(cst.port(), 0);
        assert!(tokio::net::TcpListener::bind(("127.0.0.1", port)).await.is_ok());
    }
}
