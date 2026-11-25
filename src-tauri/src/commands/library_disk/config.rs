//! Library (LH): the hooks Claude Code's settings files declare, read-only. Global is HOME's
//! `.claude/settings.json`; a project has its `.claude/settings.json` and
//! `.claude/settings.local.json`. Nothing here writes: the files are the user's and every
//! session reads them.
//!
//! A settings file can hold tokens (`env`, and `~/.claude/settings.json` is a credential file to
//! [`SecretPaths`]), so only `hooks` and `disableAllHooks` leave it, a parse error says where
//! and never what, and the command is local-only (remote/guard.rs). A file is read where the CLI
//! reads it; reached through a link, it must stay in the same HOME / project and out of a
//! credential store, as the Library's folders ([`super::folder_allowed`]).

use super::{folder_allowed, read_text, LibraryScope};
use crate::commands::memory::project_root;
use crate::db::Db;
use crate::remote::guard::SecretPaths;
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigKind {
    Hook,
}

/// The settings level an item comes from, as Claude Code names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    User,
    Project,
    Local,
}

impl ConfigSource {
    fn as_str(self) -> &'static str {
        match self {
            ConfigSource::User => "user",
            ConfigSource::Project => "project",
            ConfigSource::Local => "local",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HookInfo {
    pub event: String,
    pub matcher: Option<String>,
    /// `command`, `prompt`, … as written.
    pub hook_type: String,
    /// The command (or prompt); another type's whole entry as JSON.
    pub command: String,
    pub timeout: Option<u64>,
    /// Its file sets `disableAllHooks: true`.
    pub all_disabled: bool,
}

/// One row of the Library's Hooks tab.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigItem {
    pub kind: ConfigKind,
    /// Unique in one scan: the row's id and the `path` of the Library's URL.
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: ConfigSource,
    /// The file it is declared in, as the CLI reads it (links not followed).
    pub file: String,
    /// Why the file (or this entry) can't be shown; never its content.
    pub error: Option<String>,
    pub hook: Option<HookInfo>,
}

/// A settings file of one level, relative to HOME or the project.
struct SettingsFile {
    source: ConfigSource,
    rel: &'static str,
}

const USER_FILES: &[SettingsFile] = &[SettingsFile { source: ConfigSource::User, rel: ".claude/settings.json" }];
const PROJECT_FILES: &[SettingsFile] = &[
    SettingsFile { source: ConfigSource::Project, rel: ".claude/settings.json" },
    SettingsFile { source: ConfigSource::Local, rel: ".claude/settings.local.json" },
];

/// The JSON object at `rel` under `base`; `None` when there is no file.
fn read_json(base: &Path, rel: &str, secret: &SecretPaths) -> Result<Option<Map<String, Value>>, String> {
    let at = base.join(rel);
    if fs::symlink_metadata(&at).is_err() {
        return Ok(None);
    }
    let real_base = fs::canonicalize(base).map_err(|_| format!("Folder not found: {}", base.display()))?;
    let real = fs::canonicalize(&at).map_err(|_| format!("{} is a link to something that doesn't exist", at.display()))?;
    if !folder_allowed(&real, &real_base, Path::new(rel), secret) {
        return Err(format!("{} leads outside {} — Puppeteer won't open it", at.display(), base.display()));
    }
    if !real.is_file() {
        return Err(format!("{} is not a file", at.display()));
    }
    let text = read_text(&real)?;
    // serde_json's message quotes no input; line and column are enough to find the mistake.
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(Some(map)),
        Ok(_) => Err(format!("{} is not a JSON object", at.display())),
        Err(e) => Err(format!("{} is not valid JSON (line {}, column {})", at.display(), e.line(), e.column())),
    }
}

fn error_item(kind: ConfigKind, source: ConfigSource, file: &Path, n: usize, error: String) -> ConfigItem {
    ConfigItem {
        kind,
        id: format!("{}-error:{}:{n}", kind_str(kind), source.as_str()),
        name: file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        description: String::new(),
        source,
        file: file.display().to_string(),
        error: Some(error),
        hook: None,
    }
}

fn kind_str(kind: ConfigKind) -> &'static str {
    match kind {
        ConfigKind::Hook => "hook",
    }
}

/// The text a hook runs: its command or prompt, else the whole entry.
fn hook_text(hook_type: &str, hook: &Map<String, Value>) -> String {
    let field = match hook_type {
        "command" => "command",
        "prompt" => "prompt",
        _ => "",
    };
    match hook.get(field).and_then(Value::as_str) {
        Some(text) => text.to_string(),
        None => Value::Object(hook.clone()).to_string(),
    }
}

/// The hook rows of one settings file: one per hook entry of each event's matcher groups.
fn hooks_of(settings: &Map<String, Value>, source: ConfigSource, file: &Path, out: &mut Vec<ConfigItem>) {
    let Some(hooks) = settings.get("hooks") else { return };
    let all_disabled = settings.get("disableAllHooks").and_then(Value::as_bool) == Some(true);
    let mut errors = 0;
    let mut error = |out: &mut Vec<ConfigItem>, message: String| {
        out.push(error_item(ConfigKind::Hook, source, file, errors, message));
        errors += 1;
    };
    let Some(events) = hooks.as_object() else {
        error(out, format!("\"hooks\" in {} is not an object", file.display()));
        return;
    };
    for (event, groups) in events {
        let Some(groups) = groups.as_array() else {
            error(out, format!("\"hooks.{event}\" in {} is not a list", file.display()));
            continue;
        };
        for (i, group) in groups.iter().enumerate() {
            let matcher = group.get("matcher").and_then(Value::as_str).filter(|m| !m.is_empty()).map(str::to_string);
            let Some(entries) = group.get("hooks").and_then(Value::as_array) else {
                error(out, format!("\"hooks.{event}[{i}]\" in {} has no \"hooks\" list", file.display()));
                continue;
            };
            for (j, entry) in entries.iter().enumerate() {
                let Some(entry) = entry.as_object() else {
                    error(out, format!("\"hooks.{event}[{i}].hooks[{j}]\" in {} is not an object", file.display()));
                    continue;
                };
                let hook_type = entry.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
                let command = hook_text(&hook_type, entry);
                out.push(ConfigItem {
                    kind: ConfigKind::Hook,
                    id: format!("hook:{}:{event}:{i}:{j}", source.as_str()),
                    name: match &matcher {
                        Some(m) => format!("{event} · {m}"),
                        None => event.clone(),
                    },
                    description: command.clone(),
                    source,
                    file: file.display().to_string(),
                    error: None,
                    hook: Some(HookInfo {
                        event: event.clone(),
                        matcher: matcher.clone(),
                        hook_type,
                        command,
                        timeout: entry.get("timeout").and_then(Value::as_u64),
                        all_disabled,
                    }),
                });
            }
        }
    }
}

/// Every hook the settings files of HOME (`project` none) or of `project` declare.
pub(crate) fn scan_config_at(home: &Path, project: Option<&Path>, secret: &SecretPaths) -> Vec<ConfigItem> {
    let (base, files) = match project {
        Some(p) => (p, PROJECT_FILES),
        None => (home, USER_FILES),
    };
    let mut out = Vec::new();
    for f in files {
        let file = base.join(f.rel);
        match read_json(base, f.rel, secret) {
            Ok(Some(settings)) => hooks_of(&settings, f.source, &file, &mut out),
            Ok(None) => {}
            Err(e) => out.push(error_item(ConfigKind::Hook, f.source, &file, 0, e)),
        }
    }
    out
}

/// Local-only: hook commands can carry tokens.
#[tauri::command]
pub async fn library_config_scan(
    app: AppHandle,
    db: State<'_, Db>,
    scope: LibraryScope,
    project_id: Option<String>,
) -> Result<Vec<ConfigItem>, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let project = match scope {
        LibraryScope::Global => None,
        LibraryScope::Project => {
            Some(project_root(db.inner(), project_id.as_deref().ok_or("A project is required")?).await?)
        }
    };
    let secret = SecretPaths::for_app(&app);
    super::blocking(move || Ok(scan_config_at(&home, project.as_deref(), &secret))).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A temp HOME and project under one canonical folder.
    struct Fixture {
        dir: PathBuf,
        secret: SecretPaths,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-libconfig-{}-{}", label, uuid::Uuid::new_v4()));
            fs::create_dir_all(dir.join("home")).unwrap();
            fs::create_dir_all(dir.join("proj")).unwrap();
            let dir = fs::canonicalize(dir).unwrap();
            let home = dir.join("home");
            let app_data = home.join("Library/Application Support/io.github.v9n8.puppeteer.test");
            let secret = SecretPaths::new(Some(&home), Some(&app_data));
            Fixture { dir, secret }
        }

        fn home(&self) -> PathBuf {
            self.dir.join("home")
        }

        fn proj(&self) -> PathBuf {
            self.dir.join("proj")
        }

        fn write(&self, rel: &str, content: &str) -> PathBuf {
            let path = self.dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            path
        }

        fn global(&self) -> Vec<ConfigItem> {
            scan_config_at(&self.home(), None, &self.secret)
        }

        fn project(&self) -> Vec<ConfigItem> {
            scan_config_at(&self.home(), Some(&self.proj()), &self.secret)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    const PRE_BASH: &str = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo hi","timeout":5}]}]}}"#;

    #[test]
    fn a_user_hook_is_listed_with_its_event_matcher_and_command() {
        let f = Fixture::new("user");
        f.write("home/.claude/settings.json", PRE_BASH);
        let items = f.global();
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.kind, ConfigKind::Hook);
        assert_eq!(it.name, "PreToolUse · Bash");
        assert_eq!(it.description, "echo hi");
        assert_eq!(it.source, ConfigSource::User);
        assert!(it.error.is_none());
        let hook = it.hook.as_ref().unwrap();
        assert_eq!((hook.event.as_str(), hook.matcher.as_deref(), hook.hook_type.as_str()), ("PreToolUse", Some("Bash"), "command"));
        assert_eq!(hook.timeout, Some(5));
        assert!(!hook.all_disabled);
    }

    #[test]
    fn a_project_lists_its_shared_and_local_hooks_and_global_does_not() {
        let f = Fixture::new("project");
        f.write("proj/.claude/settings.json", PRE_BASH);
        f.write("proj/.claude/settings.local.json", r#"{"hooks":{"Stop":[{"hooks":[{"type":"prompt","prompt":"check"}]}]}}"#);
        let items = f.project();
        let rows: Vec<_> = items.iter().map(|i| (i.name.as_str(), i.source, i.description.as_str())).collect();
        assert_eq!(rows, vec![("PreToolUse · Bash", ConfigSource::Project, "echo hi"), ("Stop", ConfigSource::Local, "check")]);
        assert_ne!(items[0].id, items[1].id);
        assert!(f.global().is_empty());
    }

    #[test]
    fn disable_all_hooks_marks_the_rows_of_its_file() {
        let f = Fixture::new("disabled");
        f.write(
            "home/.claude/settings.json",
            r#"{"disableAllHooks":true,"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"}]}]}}"#,
        );
        assert!(f.global()[0].hook.as_ref().unwrap().all_disabled);
    }

    #[test]
    fn invalid_json_is_an_error_row_without_the_file_content() {
        let f = Fixture::new("invalid");
        f.write("home/.claude/settings.json", r#"{"env":{"TOKEN":"sk-secret-123"}, oops"#);
        let items = f.global();
        assert_eq!(items.len(), 1);
        let err = items[0].error.as_deref().unwrap();
        assert!(err.contains("settings.json") && err.contains("line 1"), "{err}");
        assert!(!serde_json::to_string(&items).unwrap().contains("sk-secret"));
    }

    #[test]
    fn the_env_block_never_reaches_the_scan() {
        let f = Fixture::new("env");
        f.write(
            "home/.claude/settings.json",
            r#"{"env":{"ANTHROPIC_API_KEY":"sk-secret-456"},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"}]}]}}"#,
        );
        let json = serde_json::to_string(&f.global()).unwrap();
        assert!(!json.contains("sk-secret") && !json.contains("ANTHROPIC_API_KEY"), "{json}");
    }

    #[cfg(unix)]
    #[test]
    fn a_project_settings_link_out_of_the_project_is_not_read() {
        let f = Fixture::new("link");
        let outside = f.write("elsewhere/settings.json", PRE_BASH);
        fs::create_dir_all(f.proj().join(".claude")).unwrap();
        std::os::unix::fs::symlink(&outside, f.proj().join(".claude/settings.json")).unwrap();
        let items = f.project();
        assert_eq!(items.len(), 1);
        assert!(items[0].error.as_deref().unwrap().contains("leads outside"), "{:?}", items[0].error);
        assert!(items[0].hook.is_none());
    }

    #[test]
    fn no_settings_file_means_no_rows_and_no_error() {
        let f = Fixture::new("empty");
        assert!(f.global().is_empty());
        assert!(f.project().is_empty());
        f.write("home/.claude/settings.json", r#"{"model":"x"}"#);
        assert!(f.global().is_empty());
    }

    #[test]
    fn a_malformed_entry_is_an_error_row_and_the_rest_still_lists() {
        let f = Fixture::new("malformed");
        f.write(
            "home/.claude/settings.json",
            r#"{"hooks":{"Stop":[{"matcher":"x"},{"hooks":[{"type":"command","command":"ok"}]}],"Notification":"bad"}}"#,
        );
        let items = f.global();
        let errors = items.iter().filter(|i| i.error.is_some()).count();
        let ok: Vec<_> = items.iter().filter(|i| i.error.is_none()).map(|i| i.description.as_str()).collect();
        assert_eq!((errors, ok), (2, vec!["ok"]));
        let ids: std::collections::HashSet<_> = items.iter().map(|i| &i.id).collect();
        assert_eq!(ids.len(), items.len());
    }
}
