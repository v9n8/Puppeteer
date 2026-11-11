//! Skill eval: given a few sample requests, does the model pick this skill when it should, and
//! leave it alone when it shouldn't? Ported from AWOG `apps/desktop/sidecar/src/skills/eval.ts`.
//!
//! The Doctor (`skill_doctor`) checks the shape of a skill for free. This answers the costlier
//! question: among the library's skills, is this description distinct enough for the model to load
//! it at the right time. Each case is ONE tool-less turn on the warm translate sidecar
//! (`translate::submit_prompt_metered`), on its own instance with a fresh session per case so one
//! case's answer never leaks into the next.
//!
//! Spend caps, three dimensions; the first one reached stops the run and every case not yet run is
//! `skipped`, with `stopped_by` naming the dimension:
//! - `calls`: model calls in one run.
//! - `cost`: USD. Before a call the next one is estimated (the largest call seen so far, at least
//!   the prompt's size priced by `runs::pricing`); after it the estimate is replaced by what the
//!   engine reported. So the run can overshoot the cap by at most one call.
//! - `wallclock`: the whole run; a call still running at the deadline is abandoned.
//!
//! Results are returned, not stored.

use crate::commands::translate::{self, TranslateState, TurnEvent};
use crate::db::Db;
use crate::features;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};

/// Prop gating `run_skill_eval`.
pub const FEATURE: &str = "library.skill_eval";

// Hard limits at the edge (refused, not stopped mid-run).
pub const MAX_CASES_PER_RUN: usize = 20;
pub const MAX_CASE_PROMPT_CHARS: usize = 1_000;
const MAX_CASE_ID_CHARS: usize = 64;

/// Bounds a caller can set the caps within; an unset cap takes [`DEFAULT_BUDGET`].
const MAX_COST_USD: f64 = 5.0;
const MIN_COST_USD: f64 = 0.01;
const MIN_WALLCLOCK_MS: u64 = 10_000;
const MAX_WALLCLOCK_MS: u64 = 15 * 60_000;

/// Wide enough for the 8–10 cases people type by hand, tight enough that a misclick doesn't burn
/// the quota (AWOG's defaults).
pub const DEFAULT_BUDGET: Budget = Budget { max_calls: 20, max_cost_usd: 0.5, max_wallclock_ms: 5 * 60_000 };

/// Two errors in a row are almost always the system (login expired, network down), not the case.
const MAX_CONSECUTIVE_ERRORS: u32 = 2;

// Catalogue caps, so each case's input doesn't grow with the number of skills.
const MAX_CATALOGUE_ENTRIES: usize = 60;
const MAX_CATALOGUE_DESC_CHARS: usize = 300;
const MAX_REASON_CHARS: usize = 400;
const MAX_ERROR_CHARS: usize = 400;

// Pre-call estimate: 4 characters ≈ 1 token, and a one-line JSON answer stays under 150 tokens.
const CHARS_PER_TOKEN: usize = 4;
const EST_OUTPUT_TOKENS: i64 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Expect {
    /// The model should pick this skill.
    Activate,
    /// The model should not pick this skill (another one, or none, is fine).
    Skip,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub prompt: String,
    pub expect: Expect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CaseStatus {
    Scored,
    Skipped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseResult {
    pub case_id: String,
    pub prompt: String,
    pub expect: Expect,
    /// The skill the model picked; `None` = no skill (or one not in the catalogue).
    pub chosen: Option<String>,
    pub pass: bool,
    pub reason: String,
    pub status: CaseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stop {
    Calls,
    Cost,
    Wallclock,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Budget {
    pub max_calls: u32,
    pub max_cost_usd: f64,
    pub max_wallclock_ms: u64,
}

/// The caps a caller asks for; each one unset takes the default, each one set is clamped.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BudgetInput {
    pub max_calls: Option<u32>,
    pub max_cost_usd: Option<f64>,
    pub max_wallclock_ms: Option<u64>,
}

impl BudgetInput {
    pub fn resolve(&self) -> Budget {
        let d = DEFAULT_BUDGET;
        Budget {
            max_calls: self.max_calls.map_or(d.max_calls, |n| n.clamp(1, MAX_CASES_PER_RUN as u32)),
            max_cost_usd: self
                .max_cost_usd
                .filter(|c| c.is_finite())
                .map_or(d.max_cost_usd, |c| c.clamp(MIN_COST_USD, MAX_COST_USD)),
            max_wallclock_ms: self
                .max_wallclock_ms
                .map_or(d.max_wallclock_ms, |ms| ms.clamp(MIN_WALLCLOCK_MS, MAX_WALLCLOCK_MS)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalRun {
    pub skill_id: String,
    /// The skill's folder name: the id the model answers with.
    pub skill_name: String,
    pub engine: String,
    pub model: String,
    /// Unix milliseconds.
    pub started_at: i64,
    pub finished_at: i64,
    pub results: Vec<CaseResult>,
    /// Scored cases that passed / scored cases.
    pub passed: usize,
    pub total: usize,
    pub calls: u32,
    pub cost_usd: f64,
    /// Some of `cost_usd` is an estimate (Codex reports tokens, not dollars; a failed call
    /// reports nothing).
    pub cost_estimated: bool,
    pub budget: Budget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped_by: Option<Stop>,
}

/// One answer of the model and what it cost.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    pub text: String,
    /// `None` when the engine reported no usage at all.
    pub cost_usd: Option<f64>,
    /// `cost_usd` came from the engine's own dollar figure, not from token counts.
    pub cost_reported: bool,
}

/// The model behind the eval; a fake in tests.
#[async_trait]
pub trait EvalModel: Send + Sync {
    async fn complete(&self, prompt: String) -> Result<Completion, String>;
    /// The call in flight was given up (wallclock): free whatever it holds.
    async fn abandon(&self) {}
}

/// A library skill as the catalogue shows it.
#[derive(Debug, Clone)]
pub struct CatalogueSkill {
    pub name: String,
    pub description: String,
}

// ── Pure parts ──────────────────────────────────────────────────────────────

/// Refuse a run the UI should never send: no cases, too many, empty or oversized prompts, ids
/// that are empty, too long or repeated.
pub fn validate_cases(cases: &[EvalCase]) -> Result<(), String> {
    if cases.is_empty() {
        return Err("invalid_cases: no cases".into());
    }
    if cases.len() > MAX_CASES_PER_RUN {
        return Err(format!("invalid_cases: at most {MAX_CASES_PER_RUN} cases"));
    }
    let mut ids = HashSet::new();
    for c in cases {
        let id = c.id.trim();
        if id.is_empty() || id.chars().count() > MAX_CASE_ID_CHARS {
            return Err("invalid_cases: bad case id".into());
        }
        if !ids.insert(id) {
            return Err(format!("invalid_cases: duplicate case id {id}"));
        }
        if c.prompt.trim().is_empty() {
            return Err(format!("invalid_cases: empty prompt in {id}"));
        }
        if c.prompt.chars().count() > MAX_CASE_PROMPT_CHARS {
            return Err(format!("invalid_cases: prompt of {id} is over {MAX_CASE_PROMPT_CHARS} characters"));
        }
    }
    Ok(())
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// One line per skill (`- name: description`), capped. The skill under test is always listed, at
/// its own place in the order (first would hint the answer).
pub fn build_catalogue(target: &str, skills: &[CatalogueSkill]) -> String {
    let mut others = 0;
    skills
        .iter()
        .filter(|s| {
            if s.name == target {
                return true;
            }
            others += 1;
            others < MAX_CATALOGUE_ENTRIES
        })
        .map(|s| {
            let desc = s.description.split_whitespace().collect::<Vec<_>>().join(" ");
            format!("- {}: {}", s.name, clip(&desc, MAX_CATALOGUE_DESC_CHARS))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The whole turn in one prompt: the warm session has no system prompt of its own to set.
pub fn build_prompt(catalogue: &str, request: &str) -> String {
    let catalogue = if catalogue.is_empty() { "(empty)" } else { catalogue };
    format!(
        "You are the skill router of an AI coding assistant. You are given a catalogue of available \
         skills and ONE user request.\n\
         Decide which single skill (if any) you would load to handle that request, using ONLY the \
         skill names and descriptions — exactly as you would during a normal turn.\n\
         Respond with ONLY a JSON object, no prose and no code fence:\n\
         {{\"skill\": \"<skill name or null>\", \"reason\": \"<max 25 words>\"}}\n\
         Rules:\n\
         - \"skill\" MUST be a name from the catalogue, or null when no skill applies.\n\
         - Pick at most one skill. Do not invent names.\n\
         - Judge by the description only; do not assume a skill does more than it says.\n\n\
         Skill catalogue:\n{catalogue}\n\n\
         User request:\n\"\"\"\n{request}\n\"\"\"\n\n\
         Answer with the JSON object now."
    )
}

/// The model's pick. A name not in the catalogue counts as no pick: an invented skill is not a
/// correct activation.
pub fn parse_choice(raw: &str, known: &[&str]) -> (Option<String>, String) {
    let Ok(obj) = crate::focus::ai_util::extract_json(raw) else {
        return (None, String::new());
    };
    let reason = obj.get("reason").and_then(Value::as_str).map(|r| clip(r, MAX_REASON_CHARS)).unwrap_or_default();
    let skill = obj.get("skill").and_then(Value::as_str).map(str::trim).unwrap_or("");
    if skill.is_empty() || skill.eq_ignore_ascii_case("null") || skill.eq_ignore_ascii_case("none") {
        return (None, reason);
    }
    (known.contains(&skill).then(|| skill.to_string()), reason)
}

pub fn score(expect: Expect, chosen: Option<&str>, target: &str) -> bool {
    match expect {
        Expect::Activate => chosen == Some(target),
        Expect::Skip => chosen != Some(target),
    }
}

/// Price of a call of `prompt_chars` characters, before it is made.
pub fn estimate_call_usd(model: &str, prompt_chars: usize) -> f64 {
    let input = prompt_chars.div_ceil(CHARS_PER_TOKEN) as i64;
    crate::runs::pricing::estimate_cost(model, input, EST_OUTPUT_TOKENS, Default::default(), 0)
}

/// What a finished turn cost, from its `result` message: Claude's `total_cost_usd` (the session
/// is fresh, so its total is this turn), else the token counts priced by `runs::pricing`.
pub fn result_cost(result: &Value, model: &str) -> (Option<f64>, bool) {
    if let Some(usd) = result.get("total_cost_usd").and_then(Value::as_f64) {
        return (Some(usd), true);
    }
    let Some(usage) = result.get("usage") else { return (None, false) };
    let tok = |k: &str| usage.get(k).and_then(Value::as_i64).unwrap_or(0);
    let model = result.get("model").and_then(Value::as_str).unwrap_or(model);
    let cost = crate::runs::pricing::estimate_cost(
        model,
        tok("input_tokens"),
        tok("output_tokens"),
        crate::runs::pricing::CacheWrites::from_usage(Some(usage)),
        tok("cache_read_input_tokens"),
    );
    (Some(cost), false)
}

fn skipped(c: &EvalCase) -> CaseResult {
    CaseResult {
        case_id: c.id.clone(),
        prompt: c.prompt.clone(),
        expect: c.expect,
        chosen: None,
        pass: false,
        reason: String::new(),
        status: CaseStatus::Skipped,
        error: None,
    }
}

fn failed(c: &EvalCase, error: &str) -> CaseResult {
    CaseResult { status: CaseStatus::Error, error: Some(clip(error, MAX_ERROR_CHARS)), ..skipped(c) }
}

/// What a run needs besides the model.
pub struct EvalInput<'a> {
    pub skill_name: &'a str,
    pub cases: &'a [EvalCase],
    pub catalogue: &'a [CatalogueSkill],
    /// Model id for the pre-call price estimate.
    pub price_model: &'a str,
}

/// The run itself: results, calls made, USD spent, whether part of it is estimated, and why it
/// stopped early.
pub struct Outcome {
    pub results: Vec<CaseResult>,
    pub calls: u32,
    pub cost_usd: f64,
    pub cost_estimated: bool,
    pub stopped_by: Option<Stop>,
}

/// Run every case through `model` under `budget`, one at a time.
pub async fn run_cases(model: &dyn EvalModel, input: &EvalInput<'_>, budget: Budget) -> Outcome {
    let started = Instant::now();
    let deadline = Duration::from_millis(budget.max_wallclock_ms);
    let catalogue = build_catalogue(input.skill_name, input.catalogue);
    let known: Vec<&str> = input.catalogue.iter().map(|s| s.name.as_str()).collect();

    let mut out = Outcome { results: Vec::new(), calls: 0, cost_usd: 0.0, cost_estimated: false, stopped_by: None };
    // The dearest call so far: the CLI adds its own prompt around ours, so the size of our text
    // alone underestimates a call.
    let mut dearest = 0.0_f64;
    let mut consecutive_errors = 0;

    for c in input.cases {
        if out.stopped_by.is_none() {
            let prompt = build_prompt(&catalogue, c.prompt.trim());
            let estimate = estimate_call_usd(input.price_model, prompt.chars().count()).max(dearest);
            let elapsed = started.elapsed();
            // Three gates before every call.
            out.stopped_by = if out.calls >= budget.max_calls {
                Some(Stop::Calls)
            } else if elapsed >= deadline {
                Some(Stop::Wallclock)
            } else if out.cost_usd + estimate > budget.max_cost_usd {
                Some(Stop::Cost)
            } else {
                None
            };
            if let Some(stop) = out.stopped_by {
                tracing::warn!(event = "skill_eval_budget_reached", dimension = ?stop, calls = out.calls, cost_usd = out.cost_usd);
            } else {
                out.calls += 1;
                // Counted now: a call that fails may still have been billed.
                out.cost_usd += estimate;
                let result = match tokio::time::timeout(deadline - elapsed, model.complete(prompt)).await {
                    Ok(r) => r,
                    Err(_) => {
                        model.abandon().await;
                        out.stopped_by = Some(Stop::Wallclock);
                        Err("timeout".to_string())
                    }
                };
                match result {
                    Ok(done) => {
                        consecutive_errors = 0;
                        match done.cost_usd {
                            Some(actual) => {
                                out.cost_usd += actual - estimate;
                                dearest = dearest.max(actual);
                                out.cost_estimated |= !done.cost_reported;
                            }
                            None => out.cost_estimated = true,
                        }
                        let (chosen, reason) = parse_choice(&done.text, &known);
                        out.results.push(CaseResult {
                            pass: score(c.expect, chosen.as_deref(), input.skill_name),
                            chosen,
                            reason,
                            status: CaseStatus::Scored,
                            ..skipped(c)
                        });
                    }
                    Err(e) => {
                        out.cost_estimated = true;
                        consecutive_errors += 1;
                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS && out.stopped_by.is_none() {
                            out.stopped_by = Some(Stop::Error);
                        }
                        out.results.push(failed(c, &e));
                    }
                }
                continue;
            }
        }
        out.results.push(skipped(c));
    }
    out
}

// ── The warm sidecar ────────────────────────────────────────────────────────

/// Its own warm engine (so evals never queue behind translations), a fresh session per case, and
/// one eval at a time.
#[derive(Clone)]
pub struct SkillEvalState {
    translate: TranslateState,
    busy: Arc<AtomicBool>,
}

impl Default for SkillEvalState {
    fn default() -> Self {
        SkillEvalState {
            translate: TranslateState::default()
                .with_recycle_after(1)
                .silent()
                .tracked_as(crate::procs::ProcKind::SkillEval),
            busy: Arc::default(),
        }
    }
}

impl SkillEvalState {
    /// Stop the warm eval session (app exit).
    pub async fn shutdown(&self) {
        self.translate.shutdown().await;
    }
}

/// Clears the busy flag however the run ends.
struct BusyGuard(Arc<AtomicBool>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct WarmModel {
    app: AppHandle,
    db: Db,
    state: TranslateState,
    model: String,
    turn: Mutex<Option<u64>>,
}

#[async_trait]
impl EvalModel for WarmModel {
    async fn complete(&self, prompt: String) -> Result<Completion, String> {
        let (turn, mut rx, result) = translate::submit_prompt_metered(&self.app, &self.db, &self.state, prompt).await?;
        if let Ok(mut slot) = self.turn.lock() {
            *slot = Some(turn);
        }
        let text = loop {
            match rx.recv().await {
                Some(TurnEvent::Chunk(_)) => continue,
                Some(TurnEvent::Done(text)) => break Ok(text),
                Some(TurnEvent::Error(e)) => break Err(e),
                None => break Err("the AI engine stopped unexpectedly".to_string()),
            }
        };
        // Sent before the terminal event, so it is already there (or never will be).
        let (cost_usd, cost_reported) = match result.await {
            Ok(v) => result_cost(&v, &self.model),
            Err(_) => (None, false),
        };
        Ok(Completion { text: text?, cost_usd, cost_reported })
    }

    async fn abandon(&self) {
        let turn = self.turn.lock().ok().and_then(|t| *t);
        if let Some(turn) = turn {
            self.state.abandon_turn(turn).await;
        }
    }
}

/// The library skills, by name (the catalogue's order).
async fn load_catalogue(db: &Db) -> Result<Vec<(String, CatalogueSkill)>, String> {
    let rows: Vec<(String, String, String)> = sqlx::query_as("SELECT id, name, description FROM skills ORDER BY name")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|(id, name, description)| (id, CatalogueSkill { name, description })).collect())
}

/// Remote-callable and audited (it spends quota): run `cases` against library skill `skill_id`.
#[tauri::command]
pub async fn run_skill_eval(
    app: AppHandle,
    db: State<'_, Db>,
    state: State<'_, SkillEvalState>,
    skill_id: String,
    cases: Vec<EvalCase>,
    budget: Option<BudgetInput>,
) -> Result<EvalRun, String> {
    let db = db.inner();
    features::require(db, FEATURE).await?;
    validate_cases(&cases)?;
    let skills = load_catalogue(db).await?;
    let skill_name = skills
        .iter()
        .find(|(id, _)| *id == skill_id)
        .map(|(_, s)| s.name.clone())
        .ok_or_else(|| format!("Skill not found: {skill_id}"))?;
    let catalogue: Vec<CatalogueSkill> = skills.into_iter().map(|(_, s)| s).collect();

    let (engine, model) = translate::engine_and_model(db).await?;
    // Same guardrail as a run: these calls bill the same plan.
    let account = if engine == "codex" {
        None
    } else {
        crate::commands::claude_accounts::default_runtime_account(db).await?.map(|a| a.id)
    };
    crate::commands::stats::enforce_budget(db, &engine, account.as_deref(), false).await?;

    if state.busy.swap(true, Ordering::SeqCst) {
        return Err("busy: a skill eval is already running".into());
    }
    let _busy = BusyGuard(state.busy.clone());

    let budget = budget.unwrap_or_default().resolve();
    let price_model = if model.is_empty() { engine.clone() } else { model.clone() };
    let warm = WarmModel {
        app,
        db: db.clone(),
        state: state.translate.clone(),
        model: price_model.clone(),
        turn: Mutex::new(None),
    };
    let started_at = chrono::Utc::now().timestamp_millis();
    let out = run_cases(
        &warm,
        &EvalInput { skill_name: &skill_name, cases: &cases, catalogue: &catalogue, price_model: &price_model },
        budget,
    )
    .await;
    let scored: Vec<&CaseResult> = out.results.iter().filter(|r| r.status == CaseStatus::Scored).collect();
    Ok(EvalRun {
        skill_id,
        skill_name,
        engine,
        model,
        started_at,
        finished_at: chrono::Utc::now().timestamp_millis(),
        passed: scored.iter().filter(|r| r.pass).count(),
        total: scored.len(),
        results: out.results,
        calls: out.calls,
        cost_usd: out.cost_usd,
        cost_estimated: out.cost_estimated,
        budget,
        stopped_by: out.stopped_by,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    const TARGET: &str = "pdf-filler";

    fn case(id: &str, expect: Expect) -> EvalCase {
        EvalCase { id: id.into(), prompt: format!("request {id}"), expect }
    }

    fn cases(n: usize) -> Vec<EvalCase> {
        (1..=n).map(|i| case(&format!("c{i}"), Expect::Activate)).collect()
    }

    fn catalogue() -> Vec<CatalogueSkill> {
        vec![
            CatalogueSkill { name: "commit-writer".into(), description: "Use when writing a commit message.".into() },
            CatalogueSkill { name: TARGET.into(), description: "Use when filling a PDF form.".into() },
        ]
    }

    fn budget(max_calls: u32, max_cost_usd: f64, max_wallclock_ms: u64) -> Budget {
        Budget { max_calls, max_cost_usd, max_wallclock_ms }
    }

    /// A model that answers from a script, one entry per call (the last one repeats), and counts
    /// its calls.
    struct Fake {
        script: Vec<Result<Completion, String>>,
        calls: AtomicU32,
        abandoned: AtomicBool,
        hang: bool,
    }

    impl Fake {
        fn new(script: Vec<Result<Completion, String>>) -> Self {
            Fake { script, calls: AtomicU32::new(0), abandoned: AtomicBool::new(false), hang: false }
        }
        fn picking(skill: &str, cost: f64) -> Self {
            Fake::new(vec![Ok(answer(skill, Some(cost)))])
        }
        fn calls(&self) -> u32 {
            self.calls.load(Ordering::SeqCst)
        }
    }

    fn answer(skill: &str, cost_usd: Option<f64>) -> Completion {
        Completion {
            text: format!("{{\"skill\": \"{skill}\", \"reason\": \"fits\"}}"),
            cost_usd,
            cost_reported: true,
        }
    }

    #[async_trait]
    impl EvalModel for Fake {
        async fn complete(&self, prompt: String) -> Result<Completion, String> {
            assert!(prompt.contains("User request:"));
            let n = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
            if self.hang {
                std::future::pending::<()>().await;
            }
            self.script[n.min(self.script.len() - 1)].clone()
        }
        async fn abandon(&self) {
            self.abandoned.store(true, Ordering::SeqCst);
        }
    }

    async fn run(model: &Fake, cases: &[EvalCase], budget: Budget) -> Outcome {
        let catalogue = catalogue();
        let input = EvalInput { skill_name: TARGET, cases, catalogue: &catalogue, price_model: "claude-haiku-4-5" };
        run_cases(model, &input, budget).await
    }

    fn statuses(out: &Outcome) -> Vec<CaseStatus> {
        out.results.iter().map(|r| r.status).collect()
    }

    #[tokio::test]
    async fn every_case_is_scored_under_the_caps() {
        let model = Fake::picking(TARGET, 0.001);
        let mut cs = cases(2);
        cs.push(case("neg", Expect::Skip));
        let out = run(&model, &cs, DEFAULT_BUDGET).await;
        assert_eq!(statuses(&out), vec![CaseStatus::Scored; 3]);
        assert_eq!(out.results.iter().map(|r| r.pass).collect::<Vec<_>>(), vec![true, true, false]);
        assert_eq!(out.results[0].chosen.as_deref(), Some(TARGET));
        assert_eq!(out.results[0].reason, "fits");
        assert_eq!((out.calls, out.stopped_by), (3, None));
        assert!((out.cost_usd - 0.003).abs() < 1e-9, "estimates replaced by the reported cost");
        assert!(!out.cost_estimated);
    }

    /// The manual check of the plan: a cap of 2 calls for 5 cases leaves 3 skipped.
    #[tokio::test]
    async fn the_call_cap_skips_the_rest() {
        let model = Fake::picking(TARGET, 0.001);
        let out = run(&model, &cases(5), budget(2, 5.0, 60_000)).await;
        assert_eq!(model.calls(), 2);
        assert_eq!(
            statuses(&out),
            [vec![CaseStatus::Scored; 2], vec![CaseStatus::Skipped; 3]].concat()
        );
        assert_eq!(out.stopped_by, Some(Stop::Calls));
        let skipped = &out.results[4];
        assert_eq!((skipped.chosen.as_ref(), skipped.pass, skipped.case_id.as_str()), (None, false, "c5"));
    }

    #[tokio::test]
    async fn the_cost_cap_stops_before_a_call_it_cannot_afford() {
        // $0.04 a call against $0.10: after two ($0.08) the dearest-call estimate says a third
        // would go over.
        let model = Fake::picking(TARGET, 0.04);
        let out = run(&model, &cases(5), budget(20, 0.10, 60_000)).await;
        assert_eq!(model.calls(), 2);
        assert_eq!(out.stopped_by, Some(Stop::Cost));
        assert_eq!(statuses(&out)[2..], [CaseStatus::Skipped; 3]);
        assert!(out.cost_usd <= 0.10);
    }

    #[tokio::test]
    async fn the_cost_cap_counts_the_estimate_of_the_first_call() {
        // Below the price of even our own prompt: nothing is sent.
        let model = Fake::picking(TARGET, 0.0);
        let out = run(&model, &cases(3), budget(20, 1e-9, 60_000)).await;
        assert_eq!(model.calls(), 0);
        assert_eq!(out.stopped_by, Some(Stop::Cost));
        assert_eq!(statuses(&out), vec![CaseStatus::Skipped; 3]);
    }

    #[tokio::test]
    async fn an_elapsed_wallclock_sends_nothing() {
        let model = Fake::picking(TARGET, 0.001);
        let out = run(&model, &cases(3), budget(20, 5.0, 0)).await;
        assert_eq!(model.calls(), 0);
        assert_eq!(out.stopped_by, Some(Stop::Wallclock));
        assert_eq!(statuses(&out), vec![CaseStatus::Skipped; 3]);
    }

    #[tokio::test]
    async fn a_call_still_running_at_the_deadline_is_abandoned() {
        let mut model = Fake::picking(TARGET, 0.001);
        model.hang = true;
        let out = run(&model, &cases(3), budget(20, 5.0, 50)).await;
        assert_eq!(model.calls(), 1);
        assert!(model.abandoned.load(Ordering::SeqCst));
        assert_eq!(out.stopped_by, Some(Stop::Wallclock));
        assert_eq!(statuses(&out), vec![CaseStatus::Error, CaseStatus::Skipped, CaseStatus::Skipped]);
        assert_eq!(out.results[0].error.as_deref(), Some("timeout"));
        assert!(out.cost_usd > 0.0, "the abandoned call is still counted");
    }

    #[tokio::test]
    async fn two_errors_in_a_row_stop_the_run() {
        let model = Fake::new(vec![
            Ok(answer(TARGET, Some(0.001))),
            Err("login expired".into()),
            Err("login expired".into()),
            Ok(answer(TARGET, Some(0.001))),
        ]);
        let out = run(&model, &cases(5), DEFAULT_BUDGET).await;
        assert_eq!(model.calls(), 3);
        assert_eq!(
            statuses(&out),
            vec![CaseStatus::Scored, CaseStatus::Error, CaseStatus::Error, CaseStatus::Skipped, CaseStatus::Skipped]
        );
        assert_eq!(out.stopped_by, Some(Stop::Error));
        assert!(out.cost_estimated, "failed calls keep their estimate");
    }

    #[tokio::test]
    async fn one_error_does_not_stop_the_run() {
        let model = Fake::new(vec![Err("glitch".into()), Ok(answer(TARGET, None))]);
        let out = run(&model, &cases(3), DEFAULT_BUDGET).await;
        assert_eq!(statuses(&out), vec![CaseStatus::Error, CaseStatus::Scored, CaseStatus::Scored]);
        assert_eq!(out.stopped_by, None);
        assert!(out.cost_estimated, "no usage reported");
    }

    #[test]
    fn parse_choice_reads_the_pick() {
        let known = ["pdf-filler", "commit-writer"];
        assert_eq!(
            parse_choice(r#"{"skill":"pdf-filler","reason":"form"}"#, &known),
            (Some("pdf-filler".into()), "form".into())
        );
        let fenced = "```json\n{\"skill\": \"commit-writer\", \"reason\": \"git\"}\n```";
        assert_eq!(parse_choice(fenced, &known).0.as_deref(), Some("commit-writer"));
        assert_eq!(parse_choice(r#"{"skill":null,"reason":"n/a"}"#, &known), (None, "n/a".into()));
        assert_eq!(parse_choice(r#"{"skill":"none"}"#, &known).0, None);
        assert_eq!(parse_choice(r#"{"skill":"made-up"}"#, &known).0, None, "invented names count as none");
        assert_eq!(parse_choice("I would use the pdf-filler skill.", &known), (None, String::new()));
    }

    #[test]
    fn score_by_expectation() {
        assert!(score(Expect::Activate, Some(TARGET), TARGET));
        assert!(!score(Expect::Activate, Some("commit-writer"), TARGET));
        assert!(!score(Expect::Activate, None, TARGET));
        assert!(score(Expect::Skip, None, TARGET));
        assert!(score(Expect::Skip, Some("commit-writer"), TARGET));
        assert!(!score(Expect::Skip, Some(TARGET), TARGET));
    }

    #[test]
    fn the_catalogue_is_capped_but_keeps_the_target() {
        let mut skills: Vec<CatalogueSkill> = (0..100)
            .map(|i| CatalogueSkill { name: format!("s{i:03}"), description: "x  \n y ".repeat(200) })
            .collect();
        skills.push(CatalogueSkill { name: "zz-target".into(), description: "Use when.".into() });
        let text = build_catalogue("zz-target", &skills);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_CATALOGUE_ENTRIES);
        assert_eq!(lines.last(), Some(&"- zz-target: Use when."), "kept, at its own place");
        assert!(lines[0].starts_with("- s000: x y x y"), "whitespace collapsed");
        assert!(lines[0].chars().count() <= "- s000: ".len() + MAX_CATALOGUE_DESC_CHARS);
    }

    #[test]
    fn the_prompt_wraps_the_request() {
        let p = build_prompt("- a: b", "fill this form");
        assert!(p.contains("Skill catalogue:\n- a: b"));
        assert!(p.contains("\"\"\"\nfill this form\n\"\"\""));
        assert!(build_prompt("", "x").contains("(empty)"));
    }

    #[test]
    fn cases_are_checked_at_the_edge() {
        assert!(validate_cases(&cases(MAX_CASES_PER_RUN)).is_ok());
        assert!(validate_cases(&[]).is_err());
        assert!(validate_cases(&cases(MAX_CASES_PER_RUN + 1)).is_err());
        assert!(validate_cases(&[case("a", Expect::Skip), case("a", Expect::Activate)]).is_err());
        assert!(validate_cases(&[case(" ", Expect::Skip)]).is_err());
        assert!(validate_cases(&[case(&"x".repeat(65), Expect::Skip)]).is_err());
        let mut long = case("a", Expect::Skip);
        long.prompt = "é".repeat(MAX_CASE_PROMPT_CHARS);
        assert!(validate_cases(std::slice::from_ref(&long)).is_ok());
        long.prompt.push('é');
        assert!(validate_cases(&[long]).is_err());
        let mut blank = case("a", Expect::Skip);
        blank.prompt = "  \n".into();
        assert!(validate_cases(&[blank]).is_err());
    }

    #[test]
    fn budget_input_takes_defaults_and_clamps() {
        assert_eq!(BudgetInput::default().resolve(), DEFAULT_BUDGET);
        let b = BudgetInput { max_calls: Some(0), max_cost_usd: Some(f64::NAN), max_wallclock_ms: Some(1) }.resolve();
        assert_eq!(b, budget(1, DEFAULT_BUDGET.max_cost_usd, MIN_WALLCLOCK_MS));
        let b = BudgetInput { max_calls: Some(99), max_cost_usd: Some(1e6), max_wallclock_ms: Some(u64::MAX) }.resolve();
        assert_eq!(b, budget(MAX_CASES_PER_RUN as u32, MAX_COST_USD, MAX_WALLCLOCK_MS));
        let b = BudgetInput { max_calls: Some(2), max_cost_usd: Some(0.0), max_wallclock_ms: None }.resolve();
        assert_eq!(b, budget(2, MIN_COST_USD, DEFAULT_BUDGET.max_wallclock_ms));
    }

    #[test]
    fn cost_of_a_result_prefers_the_reported_figure() {
        let claude = serde_json::json!({ "type": "result", "total_cost_usd": 0.012, "usage": { "input_tokens": 9 } });
        assert_eq!(result_cost(&claude, "haiku"), (Some(0.012), true));
        let codex = serde_json::json!({ "type": "result", "model": "gpt-5.5", "usage": { "input_tokens": 1_000_000 } });
        assert_eq!(result_cost(&codex, "codex"), (Some(5.0), false), "priced by its own model");
        assert_eq!(result_cost(&serde_json::json!({ "type": "result" }), "haiku"), (None, false));
    }

    #[test]
    fn a_call_estimate_grows_with_the_prompt() {
        let small = estimate_call_usd("claude-haiku-4-5", 1_000);
        assert!(small > 0.0);
        assert!(estimate_call_usd("claude-haiku-4-5", 100_000) > small);
        assert!(estimate_call_usd("claude-haiku-4-5", 8_000) < DEFAULT_BUDGET.max_cost_usd);
    }

    #[tokio::test]
    async fn the_prop_is_off_by_default_and_the_catalogue_is_the_library() {
        let db = crate::slack_threads::tests::memory_db().await;
        assert_eq!(features::require(&db, FEATURE).await, Err(format!("feature_disabled: {FEATURE}")), "off by default");
        crate::commands::settings::set_setting(&db, &format!("feature.{FEATURE}"), "true").await.unwrap();
        assert_eq!(features::require(&db, FEATURE).await, Ok(()));
        sqlx::query("INSERT INTO skills (id, name, description, source_path, updated_at) VALUES ('id1', 'pdf-filler', 'Use when filling a PDF form.', '/tmp/x', '2026-01-01')")
            .execute(&db)
            .await
            .unwrap();
        let skills = load_catalogue(&db).await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!((skills[0].0.as_str(), skills[0].1.name.as_str()), ("id1", "pdf-filler"));
    }
}
