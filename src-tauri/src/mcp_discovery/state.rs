//! What the MCP views show for each discovered server: its state against the MCP list, the
//! conflicts it carries, and the two sections (Settings: global; a project: its own sources and
//! the global ones).
//!
//! States: *Not managed* (no copy in the MCP list) → Import; *Managed* (a copy remembers this
//! source) → link to it; *Changed* (the copy's fingerprint is not the source's any more) → Sync;
//! *Blocked* (listed only, never importable: plugin S5, managed S6); *Unreadable* (the entry could
//! not be read). A copy matches on `source_kind` + `source_ref` + `source_name`; a connector on its
//! profile (`connector_account`) + claude.ai name, since each Claude profile manages its own.
//!
//! Conflict labels (plan 3.3) on connector rows, never acted on here: C1 the profile is the same
//! claude.ai login as global, C2 the same login as another app account, C3 the connector's run key
//! is another managed server's name (a run leaves the connector out), C4 the profile's login could
//! not be read. Only equal / not equal is known about a login: no account value is here.

use super::claude_sources::GLOBAL_PROFILE;
use super::probe::ProfileIdentity;
use super::{DiscoveredServer, DiscoveryProblem, DiscoveryReport, DiscoveryScope, SourceId};
use crate::db::Db;
use crate::import::McpSummary;
use serde::Serialize;
use std::collections::HashMap;

/// `mcp_servers.source_kind` of a claude.ai connector copy.
pub const CLAUDEAI_KIND: &str = "claudeai";

/// One `mcp_servers` row with its source columns: what a discovered row is matched against.
#[derive(Debug, Clone, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct ManagedServer {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub source_kind: Option<String>,
    pub source_ref: Option<String>,
    pub source_name: Option<String>,
    pub source_fingerprint: Option<String>,
    pub connector_account: Option<String>,
}

/// Every server of the MCP list (also those made in Puppeteer: their names count for C3).
pub async fn managed_servers(db: &Db) -> Result<Vec<ManagedServer>, String> {
    sqlx::query_as(
        "SELECT id, name, enabled, source_kind, source_ref, source_name, source_fingerprint, connector_account \
         FROM mcp_servers ORDER BY name",
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())
}

/// Labels of the app accounts: Claude (S1, S2, S3, S5 profiles) and Codex (S7 profiles).
#[derive(Debug, Clone, Default)]
pub struct ProfileLabels {
    pub claude: HashMap<String, String>,
    pub codex: HashMap<String, String>,
}

pub async fn profile_labels(db: &Db) -> Result<ProfileLabels, String> {
    let read = |table: &'static str| async move {
        sqlx::query_as::<_, (String, String)>(&format!("SELECT id, label FROM {table}"))
            .fetch_all(db)
            .await
            .map(|rows| rows.into_iter().collect::<HashMap<_, _>>())
            .map_err(|e| e.to_string())
    };
    Ok(ProfileLabels { claude: read("claude_accounts").await?, codex: read("codex_accounts").await? })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowState {
    NotManaged,
    Managed,
    Changed,
    Blocked,
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Conflict {
    C1,
    C2,
    C3,
    C4,
}

/// The profile whose runs read a row: a Claude or Codex app account, or `global`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileRef {
    #[serde(rename = "ref")]
    pub reference: String,
    /// `claude` | `codex`.
    pub engine: &'static str,
    /// The app account's label; `None` for `global` (the view names it).
    pub label: Option<String>,
}

/// A row as the webview gets it. Key names only: never an env / header VALUE, a connector's id
/// or url, or a login value; credentials in args / url arrive masked (`import::mask`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub source: SourceId,
    pub scope: DiscoveryScope,
    pub origin_label: String,
    pub profile: Option<ProfileRef>,
    pub project_id: Option<String>,
    pub app: Option<String>,
    pub name: String,
    /// The name an import gives the copy (a connector: its run key, e.g. `claude_ai_Slack`).
    pub import_name: String,
    pub transport: String,
    pub summary: Option<McpSummary>,
    pub engine_status: Option<String>,
    pub importable: bool,
    pub state: RowState,
    /// The managed copy, when there is one.
    pub server_id: Option<String>,
    pub problem: Option<String>,
    pub conflicts: Vec<Conflict>,
    /// Whether a run loads it today: managed and on app-wide (project / session switches apply
    /// on top). Unmanaged servers are never loaded.
    pub loaded_in_runs: bool,
}

/// A source that could not be read at all.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub source: SourceId,
    pub label: String,
    pub message: String,
    pub profile: Option<ProfileRef>,
    pub project_id: Option<String>,
    pub app: Option<String>,
}

/// A Claude profile and its login conflicts (shown on its connector group).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeProfile {
    #[serde(flatten)]
    pub profile: ProfileRef,
    /// `false`: C4, the login could not be read.
    pub known: bool,
    /// Profiles that are the same claude.ai login.
    pub same_login_as: Vec<String>,
    pub conflicts: Vec<Conflict>,
}

/// One section pair of the MCP views.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryView {
    pub global: Vec<Row>,
    /// Empty without a project.
    pub project: Vec<Row>,
    /// Rows a run would load but cannot, since Puppeteer does not manage them (other apps' rows,
    /// S9, are not loaded by any engine and do not count).
    pub blocked_count: usize,
    pub problems: Vec<Problem>,
    pub profiles: Vec<ClaudeProfile>,
    /// When the file sources were last read; `None` before the first scan.
    pub scanned_at: Option<String>,
    /// When the engine probes last finished; `None` before the first one.
    pub probed_at: Option<String>,
    /// Engine probes (connectors, plugin and managed servers) still running.
    pub probing: bool,
}

/// What a view is built from.
pub struct ViewInputs<'a> {
    pub report: &'a DiscoveryReport,
    pub managed: &'a [ManagedServer],
    pub identities: &'a [ProfileIdentity],
    pub labels: &'a ProfileLabels,
    /// The project section's project; `None`: the Settings view (global only).
    pub project_id: Option<&'a str>,
}

impl ViewInputs<'_> {
    fn profile(&self, source: SourceId, profile: Option<&str>) -> Option<ProfileRef> {
        let reference = profile?;
        let (engine, labels) = match source {
            SourceId::S7 => ("codex", &self.labels.codex),
            _ => ("claude", &self.labels.claude),
        };
        Some(ProfileRef { reference: reference.to_string(), engine, label: labels.get(reference).cloned() })
    }

    fn identity(&self, profile: &str) -> Option<&ProfileIdentity> {
        self.identities.iter().find(|i| i.profile == profile)
    }

    /// C1 / C2 / C4 of a Claude profile.
    fn login_conflicts(&self, profile: &str) -> Vec<Conflict> {
        let Some(identity) = self.identity(profile) else { return Vec::new() };
        let mut out = Vec::new();
        if !identity.known {
            out.push(Conflict::C4);
        }
        for other in &identity.same_login_as {
            out.push(if profile == GLOBAL_PROFILE || other == GLOBAL_PROFILE { Conflict::C1 } else { Conflict::C2 });
        }
        out.sort();
        out.dedup();
        out
    }

    fn in_view(&self, scope: DiscoveryScope, project_id: Option<&str>) -> Option<bool> {
        match scope {
            DiscoveryScope::Global => Some(true),
            // A local scope of a folder that is no project is read by no Puppeteer run.
            DiscoveryScope::Project => (self.project_id.is_some() && project_id == self.project_id).then_some(false),
        }
    }
}

/// The managed copy of `row`, if any.
pub fn managed_copy<'a>(row: &DiscoveredServer, managed: &'a [ManagedServer]) -> Option<&'a ManagedServer> {
    managed.iter().find(|m| {
        let named = m.source_name.as_deref() == Some(row.name.as_str());
        if row.source == SourceId::S1 {
            named && m.source_kind.as_deref() == Some(CLAUDEAI_KIND) && m.connector_account == row.profile
        } else {
            named && m.source_kind.as_deref() == Some(row.source.as_str()) && m.source_ref.as_deref() == Some(row.source_ref.as_str())
        }
    })
}

/// A row's state and its managed copy.
pub fn state_of<'a>(row: &DiscoveredServer, managed: &'a [ManagedServer]) -> (RowState, Option<&'a ManagedServer>) {
    let copy = managed_copy(row, managed);
    let state = if row.problem.is_some() {
        RowState::Unreadable
    } else if !row.importable {
        RowState::Blocked
    } else {
        match copy {
            None => RowState::NotManaged,
            // A connector has no launch fields to compare.
            Some(m) if row.source != SourceId::S1 && row.fingerprint.is_some() && m.source_fingerprint != row.fingerprint => {
                RowState::Changed
            }
            Some(_) => RowState::Managed,
        }
    };
    (state, copy)
}

/// C3: another server of the MCP list already has the connector's run key as its name.
pub fn key_clash(run_key: &str, own_copy: Option<&str>, managed: &[ManagedServer]) -> bool {
    managed.iter().any(|m| m.name == run_key && Some(m.id.as_str()) != own_copy)
}

fn row_of(inputs: &ViewInputs, server: &DiscoveredServer) -> Row {
    let (state, copy) = state_of(server, inputs.managed);
    let mut conflicts = Vec::new();
    if server.source == SourceId::S1 {
        if let Some(profile) = server.profile.as_deref() {
            conflicts = inputs.login_conflicts(profile);
        }
        if key_clash(&server.import_name, copy.map(|m| m.id.as_str()), inputs.managed) {
            conflicts.push(Conflict::C3);
            conflicts.sort();
        }
    }
    Row {
        id: server.id.clone(),
        source: server.source,
        scope: server.scope,
        origin_label: server.origin_label.clone(),
        profile: inputs.profile(server.source, server.profile.as_deref()),
        project_id: server.project_id.clone(),
        app: server.app.clone(),
        name: server.name.clone(),
        import_name: server.import_name.clone(),
        transport: server.transport.clone(),
        summary: server.summary.clone(),
        engine_status: server.engine_status.clone(),
        importable: server.importable,
        state,
        server_id: copy.map(|m| m.id.clone()),
        problem: server.problem.clone(),
        conflicts,
        loaded_in_runs: matches!(state, RowState::Managed | RowState::Changed) && copy.is_some_and(|m| m.enabled),
    }
}

fn problem_of(inputs: &ViewInputs, problem: &DiscoveryProblem) -> Problem {
    Problem {
        source: problem.source,
        label: problem.origin_label.clone(),
        message: problem.message.clone(),
        profile: inputs.profile(problem.source, problem.profile.as_deref()),
        project_id: problem.project_id.clone(),
        app: problem.app.clone(),
    }
}

/// A row a run would load if Puppeteer managed it, but it does not.
fn blocked(row: &Row) -> bool {
    row.source != SourceId::S9 && matches!(row.state, RowState::NotManaged | RowState::Blocked | RowState::Unreadable)
}

/// The sections of one view (times and `probing` are the cache's).
pub fn build_view(inputs: &ViewInputs) -> DiscoveryView {
    let mut view = DiscoveryView::default();
    for server in &inputs.report.servers {
        match inputs.in_view(server.scope, server.project_id.as_deref()) {
            Some(true) => view.global.push(row_of(inputs, server)),
            Some(false) => view.project.push(row_of(inputs, server)),
            None => {}
        }
    }
    let order = |r: &Row| (r.source, r.profile.as_ref().map(|p| p.reference.clone()), r.origin_label.clone(), r.name.clone());
    view.global.sort_by_key(order);
    view.project.sort_by_key(order);
    view.blocked_count = view.global.iter().chain(&view.project).filter(|r| blocked(r)).count();
    view.problems = inputs
        .report
        .problems
        .iter()
        .filter(|p| p.project_id.is_none() || (inputs.project_id.is_some() && p.project_id.as_deref() == inputs.project_id))
        .map(|p| problem_of(inputs, p))
        .collect();
    view.profiles = inputs
        .identities
        .iter()
        .map(|i| ClaudeProfile {
            profile: inputs.profile(SourceId::S1, Some(&i.profile)).expect("a profile"),
            known: i.known,
            same_login_as: i.same_login_as.clone(),
            conflicts: inputs.login_conflicts(&i.profile),
        })
        .collect();
    view
}

#[cfg(test)]
mod tests {
    use super::super::claude_sources::tests::{inputs, tmp, write};
    use super::super::probe::{merge_probes, ProbedServer};
    use super::super::{scan_files, ProjectDir};
    use super::*;

    fn identity(profile: &str, known: bool, same: &[&str]) -> ProfileIdentity {
        ProfileIdentity { profile: profile.into(), known, same_login_as: same.iter().map(|s| s.to_string()).collect() }
    }

    fn connector(name: &str) -> ProbedServer {
        ProbedServer {
            name: name.into(),
            source: "claudeai".into(),
            status: "connected".into(),
            transport: Some("claudeai-proxy".into()),
            connector: serde_json::from_value(serde_json::json!({"id": "SECRET-ID", "url": "https://SECRET-URL"})).ok(),
        }
    }

    fn labels() -> ProfileLabels {
        ProfileLabels {
            claude: [("acc-1".to_string(), "Work".to_string()), ("acc-2".to_string(), "Home".to_string())].into(),
            codex: HashMap::new(),
        }
    }

    fn copy(id: &str, name: &str, kind: &str, reference: &str, source_name: &str, fingerprint: Option<&str>, account: Option<&str>) -> ManagedServer {
        ManagedServer {
            id: id.into(),
            name: name.into(),
            enabled: true,
            source_kind: Some(kind.into()),
            source_ref: Some(reference.into()),
            source_name: Some(source_name.into()),
            source_fingerprint: fingerprint.map(str::to_string),
            connector_account: account.map(str::to_string),
        }
    }

    fn view_rows(rows: &[Row]) -> Vec<(String, RowState, Option<String>, Vec<Conflict>)> {
        rows.iter().map(|r| (r.id.clone(), r.state, r.server_id.clone(), r.conflicts.clone())).collect()
    }

    /// Connectors are per profile: global's Slack copy does not manage acc-1's, and the labels
    /// follow the identities (global = acc-1: C1; acc-2 unknown: C4).
    #[test]
    fn connectors_are_keyed_per_profile_with_login_conflicts() {
        let ok: Result<Vec<ProbedServer>, String> = Ok(vec![connector("claude.ai Slack"), connector("claude.ai Drive")]);
        let report = merge_probes([("global", &ok), ("acc-1", &ok), ("acc-2", &ok)]).into_report();
        let managed = [copy("m1", "claude_ai_Slack-global", CLAUDEAI_KIND, "global", "claude.ai Slack", None, Some("global"))];
        let identities = [identity("global", true, &["acc-1"]), identity("acc-1", true, &["global"]), identity("acc-2", false, &[])];
        let labels = labels();
        let view = build_view(&ViewInputs { report: &report, managed: &managed, identities: &identities, labels: &labels, project_id: None });
        let state = |id: &str| view.global.iter().find(|r| r.id == id).map(|r| (r.state, r.server_id.clone(), r.conflicts.clone())).unwrap();
        assert_eq!(state("S1|global|claude.ai Slack"), (RowState::Managed, Some("m1".into()), vec![Conflict::C1]));
        assert_eq!(state("S1|acc-1|claude.ai Slack"), (RowState::NotManaged, None, vec![Conflict::C1]));
        assert_eq!(state("S1|acc-2|claude.ai Slack"), (RowState::NotManaged, None, vec![Conflict::C4]));
        let slack = view.global.iter().find(|r| r.id == "S1|acc-1|claude.ai Slack").unwrap();
        assert_eq!(slack.profile, Some(ProfileRef { reference: "acc-1".into(), engine: "claude", label: Some("Work".into()) }));
        assert_eq!((slack.import_name.as_str(), slack.transport.as_str()), ("claude_ai_Slack", "connector"));
        assert!(view.global.iter().find(|r| r.id == "S1|global|claude.ai Slack").unwrap().loaded_in_runs);
        // 6 connectors, one managed.
        assert_eq!(view.blocked_count, 5);
        let profiles: Vec<_> = view.profiles.iter().map(|p| (p.profile.reference.as_str(), p.known, p.conflicts.clone())).collect();
        assert_eq!(profiles, [("global", true, vec![Conflict::C1]), ("acc-1", true, vec![Conflict::C1]), ("acc-2", false, vec![Conflict::C4])]);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("SECRET"), "{json}");
        assert!(json.contains(r#""ref":"acc-1","engine":"claude","label":"Work""#), "{json}");
    }

    #[test]
    fn two_app_accounts_with_one_login_are_c2_and_a_taken_run_key_is_c3() {
        let ok: Result<Vec<ProbedServer>, String> = Ok(vec![connector("claude.ai Slack")]);
        let report = merge_probes([("acc-1", &ok), ("acc-2", &ok)]).into_report();
        // A stdio server of the MCP list already named like the connector's run key.
        let managed = [ManagedServer { id: "u1".into(), name: "claude_ai_Slack".into(), enabled: true, ..Default::default() }];
        let identities = [identity("global", true, &[]), identity("acc-1", true, &["acc-2"]), identity("acc-2", true, &["acc-1"])];
        let labels = labels();
        let view = build_view(&ViewInputs { report: &report, managed: &managed, identities: &identities, labels: &labels, project_id: None });
        assert_eq!(
            view_rows(&view.global),
            [
                ("S1|acc-1|claude.ai Slack".into(), RowState::NotManaged, None, vec![Conflict::C2, Conflict::C3]),
                ("S1|acc-2|claude.ai Slack".into(), RowState::NotManaged, None, vec![Conflict::C2, Conflict::C3]),
            ]
        );
        assert!(view.profiles[0].conflicts.is_empty());
        // Its own copy is no clash.
        assert!(!key_clash("claude_ai_Slack", Some("u1"), &managed));
    }

    #[test]
    fn engine_only_and_unreadable_rows_are_blocked_and_a_failed_probe_is_a_problem() {
        let ok: Result<Vec<ProbedServer>, String> = Ok(vec![
            ProbedServer { name: "lint".into(), source: "plugin".into(), status: "connected".into(), transport: None, connector: None },
            ProbedServer { name: "corp".into(), source: "managed".into(), status: "connected".into(), transport: None, connector: None },
        ]);
        let failed: Result<Vec<ProbedServer>, String> = Err("no login".into());
        let report = merge_probes([("global", &ok), ("acc-1", &failed)]).into_report();
        let labels = labels();
        let view = build_view(&ViewInputs { report: &report, managed: &[], identities: &[], labels: &labels, project_id: None });
        assert_eq!(
            view_rows(&view.global),
            [("S5|global|lint".into(), RowState::Blocked, None, vec![]), ("S6|engine|corp".into(), RowState::Blocked, None, vec![])]
        );
        assert_eq!(view.blocked_count, 2);
        assert_eq!(view.problems.len(), 1);
        let p = &view.problems[0];
        assert_eq!((p.source, p.label.as_str(), p.message.as_str()), (SourceId::S1, "claude.ai", "no login"));
        assert_eq!(p.profile.as_ref().map(|p| p.label.clone()), Some(Some("Work".to_string())));
    }

    /// The files of one machine: HOME `.claude.json` (global user scope + a local scope of the
    /// project), the project's `.mcp.json`, a Cursor config of the project.
    fn machine(base: &std::path::Path) -> super::super::DiscoveryInputs {
        let mut inputs = inputs(base);
        let project = base.join("work/app");
        std::fs::create_dir_all(&project).unwrap();
        write(
            &inputs.home.join(".claude.json"),
            &format!(
                r#"{{"mcpServers": {{"x": {{"command": "x", "env": {{"X_KEY": "SECRET-X"}}}}, "gone": {{"command": "g"}}}},
                    "projects": {{"{}": {{"mcpServers": {{"loc": {{"command": "l"}}}}}}, "/elsewhere": {{"mcpServers": {{"stray": {{"command": "s"}}}}}}}}}}"#,
                project.display()
            ),
        );
        write(&project.join(".mcp.json"), r#"{"mcpServers": {"y": {"command": "y"}, "bad": 7}}"#);
        write(&project.join(".cursor/mcp.json"), r#"{"mcpServers": {"cur": {"command": "c"}}}"#);
        inputs.projects.push(ProjectDir { id: "p1".into(), path: project });
        inputs
    }

    fn managed_of(report: &DiscoveryReport, row_id: &str, server_id: &str) -> ManagedServer {
        let row = report.servers.iter().find(|r| r.id.ends_with(row_id)).unwrap();
        ManagedServer {
            id: server_id.into(),
            name: row.import_name.clone(),
            enabled: false,
            source_kind: Some(row.source.as_str().into()),
            source_ref: Some(row.source_ref.clone()),
            source_name: Some(row.name.clone()),
            source_fingerprint: row.fingerprint.clone(),
            connector_account: None,
        }
    }

    /// Settings lists the global rows only; a project lists its own sources apart and the global
    /// ones; a local scope of no project is in neither.
    #[test]
    fn project_rows_show_only_under_their_project() {
        let base = tmp("state-sections");
        let inputs = machine(&base);
        let report = scan_files(&inputs).into_report();
        let labels = labels();
        let names = |rows: &[Row]| rows.iter().map(|r| (r.source.as_str(), r.name.clone(), r.state)).collect::<Vec<_>>();

        let settings = build_view(&ViewInputs { report: &report, managed: &[], identities: &[], labels: &labels, project_id: None });
        assert_eq!(names(&settings.global), [("S2", "gone".into(), RowState::NotManaged), ("S2", "x".into(), RowState::NotManaged)]);
        assert!(settings.project.is_empty());
        assert_eq!(settings.blocked_count, 2);
        assert_eq!(settings.global[0].profile.as_ref().map(|p| p.reference.as_str()), Some("global"));

        let project = build_view(&ViewInputs { report: &report, managed: &[], identities: &[], labels: &labels, project_id: Some("p1") });
        assert_eq!(project.global.len(), 2);
        assert_eq!(
            names(&project.project),
            [
                ("S3", "loc".into(), RowState::NotManaged),
                ("S4", "bad".into(), RowState::Unreadable),
                ("S4", "y".into(), RowState::NotManaged),
                ("S9", "cur".into(), RowState::NotManaged),
            ]
        );
        // The other apps' server is not loaded by an engine: not counted.
        assert_eq!(project.blocked_count, 5);
        let other = build_view(&ViewInputs { report: &report, managed: &[], identities: &[], labels: &labels, project_id: Some("p2") });
        assert!(other.project.is_empty());
        let json = serde_json::to_string(&project).unwrap();
        assert!(!json.contains("SECRET") && json.contains(r#""env_keys":["X_KEY"]"#), "{json}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A credential another tool put in a server's args or url does not reach the webview.
    #[test]
    fn a_row_masks_credentials_in_args_and_url() {
        let base = tmp("state-mask");
        let inputs = inputs(&base);
        write(
            &inputs.home.join(".claude.json"),
            r#"{"mcpServers": {"api": {"command": "npx", "args": ["-y", "pkg", "--api-key=SECRET-ARG", "--token", "SECRET-NEXT"]},
                "web": {"url": "https://me:SECRET-PW@h.example/mcp?token=SECRET-Q"}}}"#,
        );
        let report = scan_files(&inputs).into_report();
        let labels = labels();
        let view = build_view(&ViewInputs { report: &report, managed: &[], identities: &[], labels: &labels, project_id: None });
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("SECRET"), "{json}");
        assert!(json.contains(r#""args":["-y","pkg","--api-key=[redacted]","--token","[redacted]"]"#), "{json}");
        assert!(json.contains(r#""url":"https://[redacted]@h.example/mcp?token=[redacted]""#), "{json}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A reload reflects what changed on disk: an added server is listed not managed, a removed
    /// one is gone, an edited managed one is "changed", an untouched managed one stays managed.
    #[test]
    fn a_reload_shows_added_removed_and_changed_servers() {
        let base = tmp("state-reload");
        let inputs = machine(&base);
        let before = scan_files(&inputs).into_report();
        let managed = [managed_of(&before, "|x", "mx"), managed_of(&before, "|y", "my")];
        let labels = labels();
        let view = build_view(&ViewInputs { report: &before, managed: &managed, identities: &[], labels: &labels, project_id: Some("p1") });
        let state = |v: &DiscoveryView, name: &str| v.global.iter().chain(&v.project).find(|r| r.name == name).map(|r| (r.state, r.server_id.clone()));
        assert_eq!(state(&view, "x"), Some((RowState::Managed, Some("mx".into()))));
        assert_eq!(state(&view, "y"), Some((RowState::Managed, Some("my".into()))));
        assert!(!view.global.iter().find(|r| r.name == "x").unwrap().loaded_in_runs, "imported servers start off app-wide");

        // `x` gets another arg, `gone` is removed, `new` is added; a VALUE change alone is no change.
        let project = &inputs.projects[0].path;
        write(
            &inputs.home.join(".claude.json"),
            &format!(
                r#"{{"mcpServers": {{"x": {{"command": "x", "args": ["--v2"], "env": {{"X_KEY": "SECRET-OTHER"}}}}, "new": {{"url": "https://n"}}}},
                    "projects": {{"{}": {{"mcpServers": {{"loc": {{"command": "l"}}}}}}}}}}"#,
                project.display()
            ),
        );
        write(&project.join(".mcp.json"), r#"{"mcpServers": {"y": {"command": "y", "env": {}}}}"#);
        let after = scan_files(&inputs).into_report();
        let view = build_view(&ViewInputs { report: &after, managed: &managed, identities: &[], labels: &labels, project_id: Some("p1") });
        assert_eq!(state(&view, "x"), Some((RowState::Changed, Some("mx".into()))));
        assert_eq!(state(&view, "y"), Some((RowState::Managed, Some("my".into()))));
        assert_eq!(state(&view, "new"), Some((RowState::NotManaged, None)));
        assert_eq!(state(&view, "gone"), None);
        assert_eq!(state(&view, "bad"), None);
        let _ = std::fs::remove_dir_all(&base);
    }
}
