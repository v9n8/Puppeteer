use super::*;
use Verification as V;

#[test]
fn the_last_verification_line_decides() {
    for (text, want) in [
        ("Done.\n\nVerification: PASS — cargo test green", V::Pass),
        ("Done.\nVerification: FAIL — 2 tests red", V::Fail),
        ("Docs only.\nVerification: NONE — no code changed", V::None),
        ("Verification: pass - lint ok", V::Pass),
        ("  Verification:FAIL—build broke", V::Fail),
        ("**Verification:** PASS — all green", V::Pass),
        ("**Verification: FAIL** — 1 red", V::Fail),
        // An earlier line is overruled by the last one.
        ("Verification: FAIL — red\nfixed it\nVerification: PASS — green", V::Pass),
        // Text after the line does not hide it.
        ("Verification: NONE — docs\n\nThanks!", V::None),
    ] {
        assert_eq!(parse_verification(text), want, "{text}");
    }
}

#[test]
fn no_line_or_the_old_free_form_wording_is_missing() {
    for text in [
        "",
        "All done, tests pass.",
        "Verification: ran cargo test, all green",
        "Verification: PASSED",
        "- Verification: PASS",
        "verification: PASS",
    ] {
        assert_eq!(parse_verification(text), V::Missing, "{text}");
    }
    assert_eq!(V::Missing.as_str(), "missing");
}

/// QO-AC48: a new worker is asked for the fixed form; one made before
/// resumes with the wording stored on its row, not the new one.
#[tokio::test]
async fn a_worker_made_before_keeps_its_stored_wording() {
    let new = crate::conductor::worker_instruction(Some("You review."));
    assert!(new.ends_with(crate::conductor::WORKER_VERIFICATION));
    assert!(new.contains("`Verification: PASS — <what you ran>`"));

    let db = crate::commands::response_styles::tests::test_db().await;
    let old = "You review.\n\nBefore you finish a turn in which you changed files, run the project's tests or \
checks (test, lint, build) for the files you changed. End your reply with a `Verification:` line saying what you \
ran and the result. If no check applies, say why on that line.";
    sqlx::query(
        "INSERT INTO runs (id, project_id, type, engine, role, conductor_run_id, system_instruction, created_at) \
         VALUES ('w', 'p1', 'session', 'claude', 'worker', 'c', ?, 'c')",
    )
    .bind(old)
    .execute(&db)
    .await
    .unwrap();
    let resumed = crate::conductor::commands::resume_framing(&db, None, "w").await.unwrap();
    assert_eq!(resumed.framing.as_deref(), Some(old));
}
// ---- the stream recorder ----------------------------------------------

const RUN4: &str = include_str!("../fixtures/run4.jsonl");
const RUN5: &str = include_str!("../fixtures/run5.jsonl");

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct Row {
    kind: String,
    decided_by: String,
    agent_type: Option<String>,
    task_kind: String,
    requested_model: Option<String>,
    resolved_model: Option<String>,
    requested_effort: Option<String>,
    effort_source: String,
    policy_version: Option<i64>,
    description: Option<String>,
    status: Option<String>,
    total_tokens: Option<i64>,
    duration_ms: Option<i64>,
    escalated: i64,
}

async fn recorder_db() -> Db {
    let db = crate::commands::response_styles::tests::test_db().await;
    sqlx::query("INSERT INTO runs (id, project_id, type, engine, created_at, routing_policy_version) VALUES ('r1', 'p1', 'session', 'claude', 'c', 1)")
        .execute(&db)
        .await
        .unwrap();
    db
}

async fn feed(recorder: &mut Recorder, lines: &str) {
    for line in lines.lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        recorder.observe(Some(&v)).await;
    }
}

async fn rows(db: &Db) -> Vec<Row> {
    sqlx::query_as(
        "SELECT kind, decided_by, agent_type, task_kind, requested_model, resolved_model, requested_effort, effort_source, \
         policy_version, description, status, total_tokens, duration_ms, escalated FROM routing_decisions ORDER BY created_at, rowid",
    )
    .fetch_all(db)
    .await
    .unwrap()
}

fn agent_call(id: &str, input: Value) -> Value {
    serde_json::json!({
        "type": "assistant",
        "message": {"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": "Agent", "input": input}]},
        "parent_tool_use_id": null
    })
}

/// QO-AC42: the agent's own pick, read from a real stream (spike run 5).
#[tokio::test]
async fn a_call_with_model_and_effort_is_the_agents_decision() {
    let db = recorder_db().await;
    let mut recorder = Recorder::new(&db, "r1", "claude");
    feed(&mut recorder, RUN5).await;
    let rows = rows(&db).await;
    assert_eq!(rows.len(), 3, "the sub-agents' own Bash / Read calls are not recorded");
    assert_eq!(
        rows[0],
        Row {
            kind: "subagent".into(),
            decided_by: "agent".into(),
            agent_type: Some("Explore".into()),
            task_kind: "search".into(),
            requested_model: Some("sonnet".into()),
            resolved_model: Some("claude-sonnet-5-5".into()),
            requested_effort: Some("low".into()),
            effort_source: "param".into(),
            policy_version: Some(1),
            description: Some("[search] find parseConfig definition".into()),
            status: Some("completed".into()),
            total_tokens: rows[0].total_tokens,
            duration_ms: rows[0].duration_ms,
            escalated: 0,
        }
    );
    let kinds: Vec<_> = rows.iter().map(|r| (r.task_kind.as_str(), r.requested_effort.as_deref())).collect();
    assert_eq!(kinds, vec![("search", Some("low")), ("docs", Some("medium")), ("review", Some("medium"))]);
    // Background agents: closed by their notification, with its usage.
    assert!(rows.iter().all(|r| r.status.as_deref() == Some("completed") && r.total_tokens.is_some_and(|t| t > 0)));
}

/// QO-AC43: no model, no effort → the default; the effort's source is the
/// env while Settings › Sub-agent effort gives one, else unknown.
#[tokio::test]
async fn a_call_with_neither_is_the_default() {
    let db = recorder_db().await;
    let call = agent_call("t1", serde_json::json!({"description": "look around", "prompt": "p"}));
    let mut recorder = Recorder::new(&db, "r1", "claude");
    recorder.observe(Some(&call)).await;
    crate::commands::settings::set_setting(&db, crate::runs::subagent_env::SETTING_EFFORT, "inherit").await.unwrap();
    let mut recorder = Recorder::new(&db, "r1", "claude");
    recorder.observe(Some(&agent_call("t2", serde_json::json!({"description": "look again", "prompt": "p"})))).await;
    let rows = rows(&db).await;
    let got: Vec<_> = rows.iter().map(|r| (r.decided_by.as_str(), r.effort_source.as_str(), r.task_kind.as_str())).collect();
    assert_eq!(got, vec![("default", "env", "other"), ("default", "unknown", "other")]);
    assert_eq!(rows[0].agent_type.as_deref(), Some("general-purpose"));
    assert_eq!(rows[0].status, None, "still open");
}

/// QO-AC44 (a call in the foreground, spike run 4) and the model a hook
/// put in place of the one asked for.
#[tokio::test]
async fn a_completed_result_closes_the_call_with_the_model_that_ran() {
    let db = recorder_db().await;
    let mut recorder = Recorder::new(&db, "r1", "claude");
    feed(&mut recorder, RUN4).await;
    let run4 = rows(&db).await;
    let a = &run4[0];
    assert_eq!((a.description.as_deref(), a.decided_by.as_str()), (Some("Spike A"), "agent"));
    assert_eq!((a.requested_model.as_deref(), a.resolved_model.as_deref()), (Some("sonnet"), Some("claude-haiku-5-5")));
    assert_eq!((a.status.as_deref(), a.total_tokens, a.duration_ms), (Some("completed"), Some(44666), Some(1035)));
    let b = &run4[1];
    assert_eq!((b.resolved_model.as_deref(), b.status.as_deref(), b.total_tokens), (Some("claude-haiku-5-5"), Some("completed"), Some(42874)));

    let db = recorder_db().await;
    let mut recorder = Recorder::new(&db, "r1", "claude");
    recorder.observe(Some(&agent_call("t1", serde_json::json!({"description": "[test] run it", "model": "sonnet"})))).await;
    let result = serde_json::json!({
        "type": "user", "parent_tool_use_id": null,
        "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]},
        "tool_use_result": {"status": "completed", "resolvedModel": "claude-sonnet-5-5", "totalTokens": 12000, "totalDurationMs": 900}
    });
    recorder.observe(Some(&result)).await;
    let row = &rows(&db).await[0];
    assert_eq!((row.status.as_deref(), row.resolved_model.as_deref(), row.total_tokens), (Some("completed"), Some("claude-sonnet-5-5"), Some(12000)));
}

/// QO-AC45: a background agent's notification says how it ended.
#[tokio::test]
async fn a_notification_closes_a_background_call_once() {
    let db = recorder_db().await;
    let mut recorder = Recorder::new(&db, "r1", "claude");
    recorder.observe(Some(&agent_call("bg", serde_json::json!({"description": "[implement] x", "run_in_background": true})))).await;
    let note = |status: &str, tokens: i64| {
        serde_json::json!({"type": "system", "subtype": "task_notification", "tool_use_id": "bg", "status": status,
            "usage": {"total_tokens": tokens, "duration_ms": 50}})
    };
    recorder.observe(Some(&note("failed", 777))).await;
    recorder.observe(Some(&note("completed", 999))).await;
    let row = &rows(&db).await[0];
    assert_eq!((row.status.as_deref(), row.total_tokens, row.duration_ms), (Some("failed"), Some(777), Some(50)));
}

#[tokio::test]
async fn an_error_result_fails_the_call_and_the_rest_stop_at_the_end() {
    let db = recorder_db().await;
    let mut recorder = Recorder::new(&db, "r1", "claude");
    recorder.observe(Some(&agent_call("e", serde_json::json!({"description": "a"})))).await;
    recorder.observe(Some(&agent_call("left", serde_json::json!({"description": "b"})))).await;
    let error = serde_json::json!({
        "type": "user", "parent_tool_use_id": null,
        "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "e", "is_error": true, "content": "no"}]}
    });
    recorder.observe(Some(&error)).await;
    recorder.close().await;
    let got: Vec<_> = rows(&db).await.into_iter().map(|r| r.status).collect();
    assert_eq!(got, vec![Some("failed".into()), Some("stopped".into())]);
}

/// QO-AC46: the same work asked again one tier up is an escalation.
#[tokio::test]
async fn the_same_work_one_tier_up_escalates_the_earlier_call() {
    let db = recorder_db().await;
    let mut recorder = Recorder::new(&db, "r1", "claude");
    let call = |id: &str, description: &str, model: &str, effort: &str| {
        agent_call(id, serde_json::json!({"description": description, "model": model, "effort": effort}))
    };
    recorder.observe(Some(&call("a", "[review] Check  auth diff", "sonnet", "medium"))).await;
    // Not higher: a plain second call.
    recorder.observe(Some(&call("b", "[review] check auth diff", "sonnet", "low"))).await;
    recorder.observe(Some(&call("c", "[REVIEW] check auth   diff ", "opus", "low"))).await;
    let got: Vec<_> = rows(&db).await.into_iter().map(|r| (r.decided_by, r.escalated)).collect();
    assert_eq!(got, vec![("agent".into(), 0), ("agent".into(), 1), ("escalation".into(), 0)]);
    // Another drain is another turn: not an escalation.
    let mut next = Recorder::new(&db, "r1", "claude");
    next.observe(Some(&call("d", "[review] check auth diff", "opus", "high"))).await;
    assert_eq!(rows(&db).await[3].decided_by, "agent");
}

/// The ceiling hook's report, read before or after the call it lowered.
#[tokio::test]
async fn a_clamp_is_stored_on_its_call_in_either_order() {
    let db = recorder_db().await;
    let mut recorder = Recorder::new(&db, "r1", "claude");
    let clamp = |id: &str| serde_json::json!({"type": "_puppeteer_routing_clamp", "tool_use_id": id, "from": "opus", "to": "sonnet"});
    recorder.observe(Some(&clamp("early"))).await;
    recorder.observe(Some(&agent_call("early", serde_json::json!({"description": "[review] a", "model": "opus"})))).await;
    recorder.observe(Some(&agent_call("late", serde_json::json!({"description": "[review] b", "model": "opus"})))).await;
    recorder.observe(Some(&agent_call("none", serde_json::json!({"description": "[review] c", "model": "haiku"})))).await;
    recorder.observe(Some(&clamp("late"))).await;
    // Half a report, or one for a call never recorded (a nested agent's).
    recorder.observe(Some(&serde_json::json!({"type": "_puppeteer_routing_clamp", "tool_use_id": "none", "from": "opus"}))).await;
    recorder.observe(Some(&clamp("nested"))).await;
    let got: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT tool_use_id, clamp FROM routing_decisions ORDER BY rowid").fetch_all(&db).await.unwrap();
    let want = [("early", Some("opus→sonnet")), ("late", Some("opus→sonnet")), ("none", None)];
    assert_eq!(got, want.map(|(id, c)| (id.to_string(), c.map(str::to_string))));
    assert_eq!(rows(&db).await[0].requested_model.as_deref(), Some("opus"), "the model asked for stays");
}

/// QO-AC50: no column can hold a prompt, and none of it is stored.
#[tokio::test]
async fn no_prompt_is_stored() {
    let db = recorder_db().await;
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('routing_decisions')").fetch_all(&db).await.unwrap();
    assert!(!columns.is_empty() && columns.iter().all(|c| !c.contains("prompt")), "{columns:?}");
    let mut recorder = Recorder::new(&db, "r1", "claude");
    let long = "x".repeat(200);
    recorder.observe(Some(&agent_call("t", serde_json::json!({"description": long, "prompt": "SECRET PROMPT"})))).await;
    let dump: String = sqlx::query_scalar("SELECT json_group_array(json_array(description, agent_type, requested_model)) FROM routing_decisions")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(!dump.contains("SECRET"));
    assert_eq!(rows(&db).await[0].description.as_deref().map(|d| d.chars().count()), Some(80));
}

#[tokio::test]
async fn codex_and_nested_calls_are_not_recorded() {
    let db = recorder_db().await;
    let mut codex = Recorder::new(&db, "r1", "codex");
    feed(&mut codex, RUN5).await;
    let mut nested = Recorder::new(&db, "r1", "claude");
    let mut call = agent_call("n", serde_json::json!({"description": "inner"}));
    call["parent_tool_use_id"] = Value::String("toolu_parent".into());
    nested.observe(Some(&call)).await;
    nested.observe(None).await;
    assert!(rows(&db).await.is_empty());
    let task = serde_json::json!({"type": "assistant", "parent_tool_use_id": null,
        "message": {"content": [{"type": "tool_use", "id": "old", "name": "Task", "input": {"description": "[plan] x"}}]}});
    assert_eq!(parse(&task).len(), 1, "the older tool name too");
}

/// QO-AC51: only a worker whose last turn said FAIL is held for a retry.
#[tokio::test]
async fn a_workers_failed_turn_is_remembered_for_the_hold() {
    let db = recorder_db().await;
    sqlx::query("UPDATE runs SET role = 'worker' WHERE id = 'r1'").execute(&db).await.unwrap();
    let mut recorder = Recorder::new(&db, "r1", "claude");
    let result = |text: &str| serde_json::json!({"type": "result", "subtype": "success", "result": text});
    recorder.observe(Some(&result("Verification: FAIL — red"))).await;
    assert!(recorder.failed);
    recorder.observe(Some(&result("Verification: PASS"))).await;
    assert!(!recorder.failed, "the last turn wins");
    sqlx::query("UPDATE runs SET role = NULL WHERE id = 'r1'").execute(&db).await.unwrap();
    let mut top = Recorder::new(&db, "r1", "claude");
    top.observe(Some(&result("Verification: FAIL"))).await;
    assert!(!top.failed, "not a worker");
}

/// The stream only passes through the drain, so its source is checked: one
/// recorder per drain, fed every stdout line, closed when the loop ends,
/// before the run's status is written (and the conductor told).
#[test]
fn the_drain_feeds_the_recorder() {
    let src = include_str!("../../sidecar.rs").replace("\r\n", "\n");
    let drain = &src[src.find("pub async fn drain_sidecar(").unwrap()..src.find("async fn drain_trailing_lines<R>(").unwrap()];
    let new = drain.find("let mut routing = crate::runs::routing::record::Recorder::new(&db_pool, &run_id, &engine);").unwrap();
    let observe = drain.find("routing.observe(parsed.as_ref()).await;").unwrap();
    let close = drain.find("routing.close().await;").unwrap();
    let hold = drain.find("routing.hold_if_failed(&app);").unwrap();
    let status = drain.find("UPDATE runs SET status = ?").unwrap();
    let notify = drain.rfind("cst.notify_run_finished(run_id.clone());").unwrap();
    assert!(new < observe && observe < close && close < hold && hold < status && status < notify);
    assert!(drain[..observe].ends_with("let parsed = serde_json::from_str::<Value>(&l).ok();\n                        "));
    assert_eq!(src.matches("routing.observe(").count(), 1);
}
