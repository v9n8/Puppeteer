//! Library (LH): the hooks and plugins Claude Code's settings files declare, read-only. Global is
//! HOME's `.claude/settings.json`; a project has its `.claude/settings.json` and
//! `.claude/settings.local.json`. Plugins add what `~/.claude/plugins/` holds ([`plugins`]).
//! Nothing here writes: the files are the user's and every session reads them.
//!
//! A settings file can hold tokens (`env`, and `~/.claude/settings.json` is a credential file to
//! [`SecretPaths`]), so only `hooks`, `disableAllHooks` and `enabledPlugins` leave it, a parse
//! error says where
//! and never what, and the command is local-only (remote/guard.rs). A file is read where the CLI
//! reads it; reached through a link, it must stay in the same HOME / project and out of a
//! credential store, as the Library's folders ([`super::folder_allowed`]).

use super::{folder_allowed, read_text, LibraryScope};
use crate::commands::memory::{hash, project_root};
use crate::db::Db;
use crate::remote::guard::{SecretPaths, SECRET_HOME_FILES, SECRET_HOME_FILE_PREFIXES};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager, State};

mod plugins;
pub use plugins::PluginInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigKind {
    Hook,
    Plugin,
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
    /// Seconds, as written (`1.5` too).
    pub timeout: Option<f64>,
    /// Its file sets `disableAllHooks: true`.
    pub all_disabled: bool,
}

/// One row of the Library's Hooks or Plugins tab.
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
    pub plugin: Option<PluginInfo>,
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

/// How far a link at the file may lead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// As the Library's folders: inside the base and out of a credential store.
    Library,
    /// HOME's own file, often a dotfiles link: anywhere inside HOME but a credential store or
    /// file, as the CLI follows it. `~/.claude/settings.json` is itself a credential file to
    /// [`SecretPaths`], and so, by identity and by its stored canonical form, is its link's
    /// target: the target is checked by its own path ([`credential_file_at`]), its folder in
    /// full. Only the keys above ever leave it.
    Home,
}

/// Whether `real` (canonical) is, by its own path, one of the guard's credential files in HOME
/// (`real_home`, canonical). Compared as written, case-insensitively as the guard does.
fn credential_file_at(real_home: &Path, real: &Path) -> bool {
    let Ok(rel) = real.strip_prefix(real_home) else { return false };
    let rel: Vec<String> = rel.iter().map(|c| c.to_string_lossy().to_lowercase()).collect();
    SECRET_HOME_FILES.iter().any(|f| f.split('/').eq(rel.iter().map(String::as_str)))
        || (rel.len() == 1 && SECRET_HOME_FILE_PREFIXES.iter().any(|pre| rel[0].starts_with(pre)))
}

/// The JSON object at `rel` under `base`; `None` when there is no file.
fn read_json(base: &Path, rel: &str, reach: Reach, secret: &SecretPaths) -> Result<Option<Map<String, Value>>, String> {
    let at = base.join(rel);
    if fs::symlink_metadata(&at).is_err() {
        return Ok(None);
    }
    let real_base = fs::canonicalize(base).map_err(|_| format!("Folder not found: {}", base.display()))?;
    let real = fs::canonicalize(&at).map_err(|_| format!("{} is a link to something that doesn't exist", at.display()))?;
    let allowed = folder_allowed(&real, &real_base, Path::new(rel), secret)
        || (reach == Reach::Home
            && real.starts_with(&real_base)
            && !credential_file_at(&real_base, &real)
            && real.parent().is_some_and(|dir| !secret.denies(dir)));
    if !allowed {
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
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    ConfigItem {
        kind,
        id: format!("{}-error:{}:{}:{n}", kind_str(kind), source.as_str(), name),
        name,
        description: String::new(),
        source,
        file: file.display().to_string(),
        error: Some(error),
        hook: None,
        plugin: None,
    }
}

fn kind_str(kind: ConfigKind) -> &'static str {
    match kind {
        ConfigKind::Hook => "hook",
        ConfigKind::Plugin => "plugin",
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
    // A hook's id is its content's, so a refresh after an edit elsewhere in the file keeps it;
    // the same hook twice in one event gets a count.
    let mut seen: HashMap<String, usize> = HashMap::new();
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
                let content = hash(format!("{}\0{hook_type}\0{command}", matcher.as_deref().unwrap_or_default()).as_bytes());
                let key = format!("hook:{}:{event}:{}", source.as_str(), &content[..12]);
                let n = seen.entry(key.clone()).or_default();
                *n += 1;
                out.push(ConfigItem {
                    kind: ConfigKind::Hook,
                    id: format!("{key}:{n}"),
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
                        timeout: entry.get("timeout").and_then(Value::as_f64),
                        all_disabled,
                    }),
                    plugin: None,
                });
            }
        }
    }
}

/// Every hook and plugin the settings files of HOME (`project` none) or of `project` declare;
/// plugin details come from HOME's `.claude/plugins/` either way.
pub(crate) fn scan_config_at(home: &Path, project: Option<&Path>, secret: &SecretPaths) -> Vec<ConfigItem> {
    let (base, files) = match project {
        Some(p) => (p, PROJECT_FILES),
        None => (home, USER_FILES),
    };
    let mut out = Vec::new();
    let mut levels = Vec::new();
    let mut unread = Vec::new();
    let reach = if project.is_some() { Reach::Library } else { Reach::Home };
    for f in files {
        let file = base.join(f.rel);
        match read_json(base, f.rel, reach, secret) {
            Ok(Some(settings)) => {
                hooks_of(&settings, f.source, &file, &mut out);
                match plugins::enabled_of(&settings, &file) {
                    Ok(enabled) => levels.push(plugins::Level { source: f.source, file, enabled }),
                    Err(e) => {
                        out.push(error_item(ConfigKind::Plugin, f.source, &file, 0, e));
                        unread.push(plugins::Unread { source: f.source, file: file.clone() });
                    }
                }
            }
            Ok(None) => {}
            // The file holds both: each tab says it couldn't be read.
            Err(e) => {
                out.push(error_item(ConfigKind::Hook, f.source, &file, 0, e.clone()));
                out.push(error_item(ConfigKind::Plugin, f.source, &file, 0, e));
                unread.push(plugins::Unread { source: f.source, file });
            }
        }
    }
    plugins::plugins_of(home, project.is_none(), &levels, &unread, secret, &mut out);
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
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-libconfig-{}-{}", label, uuid::Uuid::new_v4()));
            fs::create_dir_all(dir.join("home")).unwrap();
            fs::create_dir_all(dir.join("proj")).unwrap();
            Fixture { dir: fs::canonicalize(dir).unwrap() }
        }

        /// Made per scan, as the command does: its identity checks see the files there now.
        fn secret(&self) -> SecretPaths {
            let home = self.home();
            SecretPaths::new(Some(&home), Some(&home.join("Library/Application Support/io.github.v9n8.puppeteer.test")))
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
            scan_config_at(&self.home(), None, &self.secret())
        }

        fn project(&self) -> Vec<ConfigItem> {
            scan_config_at(&self.home(), Some(&self.proj()), &self.secret())
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
        assert_eq!(hook.timeout, Some(5.0));
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
    fn invalid_json_is_an_error_row_in_both_tabs_without_the_file_content() {
        let f = Fixture::new("invalid");
        f.write("home/.claude/settings.json", r#"{"env":{"TOKEN":"sk-secret-123"}, oops"#);
        let items = f.global();
        let kinds: Vec<_> = items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, vec![ConfigKind::Hook, ConfigKind::Plugin]);
        for it in &items {
            let err = it.error.as_deref().unwrap();
            assert!(err.contains("settings.json") && err.contains("line 1"), "{err}");
        }
        assert_ne!(items[0].id, items[1].id);
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
        assert_eq!(items.len(), 2);
        for it in &items {
            assert!(it.error.as_deref().unwrap().contains("leads outside"), "{:?}", it.error);
            assert!(it.hook.is_none() && it.plugin.is_none());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_user_settings_file_linked_from_a_dotfiles_folder_in_home_is_read() {
        let f = Fixture::new("dotfiles");
        let target = f.write("home/dotfiles/claude/settings.json", PRE_BASH);
        fs::create_dir_all(f.home().join(".claude")).unwrap();
        std::os::unix::fs::symlink(&target, f.home().join(".claude/settings.json")).unwrap();
        let items = f.global();
        assert_eq!(items.len(), 1, "{items:?}");
        assert!(items[0].error.is_none(), "{:?}", items[0].error);
        assert_eq!(items[0].name, "PreToolUse · Bash");
    }

    #[cfg(unix)]
    #[test]
    fn a_user_settings_link_out_of_home_is_not_read() {
        let f = Fixture::new("dotfiles-out");
        let target = f.write("elsewhere/settings.json", PRE_BASH);
        fs::create_dir_all(f.home().join(".claude")).unwrap();
        std::os::unix::fs::symlink(&target, f.home().join(".claude/settings.json")).unwrap();
        let items = f.global();
        assert!(items.iter().all(|i| i.error.as_deref().is_some_and(|e| e.contains("leads outside"))), "{items:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_user_settings_link_into_a_credential_store_is_not_read() {
        let f = Fixture::new("dotfiles-store");
        let target = f.write("home/.aws/settings.json", PRE_BASH);
        fs::create_dir_all(f.home().join(".claude")).unwrap();
        std::os::unix::fs::symlink(&target, f.home().join(".claude/settings.json")).unwrap();
        let items = f.global();
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| i.error.as_deref().is_some_and(|e| e.contains("leads outside"))), "{items:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_user_settings_link_to_a_credential_file_is_not_read() {
        let f = Fixture::new("dotfiles-credential-file");
        let target = f.write("home/.netrc", PRE_BASH);
        fs::create_dir_all(f.home().join(".claude")).unwrap();
        std::os::unix::fs::symlink(&target, f.home().join(".claude/settings.json")).unwrap();
        let items = f.global();
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| i.error.as_deref().is_some_and(|e| e.contains("leads outside"))), "{items:?}");
    }

    #[test]
    fn a_hook_keeps_its_id_when_an_earlier_one_is_removed() {
        let f = Fixture::new("stable-id");
        let two = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"a"}]},{"hooks":[{"type":"command","command":"b"}]}]}}"#;
        f.write("home/.claude/settings.json", two);
        let before = f.global().into_iter().find(|i| i.description == "b").unwrap().id;
        f.write("home/.claude/settings.json", r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"b"}]}]}}"#);
        let after = f.global();
        assert_eq!(after[0].id, before);
        // The same hook twice keeps two ids.
        let twice = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"b"},{"type":"command","command":"b"}]}]}}"#;
        f.write("home/.claude/settings.json", twice);
        let ids: std::collections::HashSet<_> = f.global().into_iter().map(|i| i.id).collect();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn a_fractional_timeout_is_kept() {
        let f = Fixture::new("timeout");
        f.write("home/.claude/settings.json", r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x","timeout":1.5}]}]}}"#);
        assert_eq!(f.global()[0].hook.as_ref().unwrap().timeout, Some(1.5));
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

    // ── Plugins ─────────────────────────────────────────────────────────────

    fn plugins(items: &[ConfigItem]) -> Vec<&ConfigItem> {
        items.iter().filter(|i| i.kind == ConfigKind::Plugin).collect()
    }

    /// Installs `key` in HOME's plugin cache and lists it in `installed_plugins.json` with the others.
    fn install(f: &Fixture, keys: &[&str]) {
        let mut plugins = Map::new();
        for key in keys {
            let (name, market) = key.split_once('@').unwrap();
            let path = f.home().join(format!(".claude/plugins/cache/{market}/{name}/1.0.0"));
            fs::create_dir_all(&path).unwrap();
            plugins.insert(
                key.to_string(),
                serde_json::json!([{ "scope": "user", "installPath": path, "version": "1.0.0" }]),
            );
        }
        let file = serde_json::json!({ "version": 2, "plugins": plugins });
        f.write("home/.claude/plugins/installed_plugins.json", &file.to_string());
    }

    #[test]
    fn global_lists_installed_and_enabled_plugins_with_their_state() {
        let f = Fixture::new("plugins-global");
        install(&f, &["a@m", "c@m"]);
        f.write("home/.claude/settings.json", r#"{"enabledPlugins":{"a@m":true,"b@m":false}}"#);
        let items = f.global();
        let rows: Vec<_> = plugins(&items)
            .iter()
            .map(|i| {
                let p = i.plugin.as_ref().unwrap();
                (p.key.as_str(), i.name.as_str(), p.enabled, p.installed, i.source)
            })
            .collect();
        assert_eq!(rows, vec![
            ("a@m", "a", Some(true), true, ConfigSource::User),
            ("b@m", "b", Some(false), false, ConfigSource::User),
            ("c@m", "c", None, true, ConfigSource::User),
        ]);
        let c = plugins(&items)[2];
        assert!(c.file.ends_with("installed_plugins.json"), "{}", c.file);
        assert_eq!(c.plugin.as_ref().unwrap().marketplace.as_deref(), Some("m"));
        assert!(items.iter().all(|i| i.error.is_none()), "{items:?}");
    }

    #[test]
    fn a_project_lists_only_its_own_plugins_and_local_wins() {
        let f = Fixture::new("plugins-project");
        install(&f, &["a@m", "x@m"]);
        f.write("home/.claude/settings.json", r#"{"enabledPlugins":{"x@m":true}}"#);
        f.write("proj/.claude/settings.json", r#"{"enabledPlugins":{"a@m":true}}"#);
        f.write("proj/.claude/settings.local.json", r#"{"enabledPlugins":{"a@m":false}}"#);
        let items = f.project();
        let rows: Vec<_> = plugins(&items)
            .iter()
            .map(|i| (i.plugin.as_ref().unwrap().key.as_str(), i.plugin.as_ref().unwrap().enabled, i.source, i.plugin.as_ref().unwrap().installed))
            .collect();
        assert_eq!(rows, vec![("a@m", Some(false), ConfigSource::Local, true)]);
    }

    #[test]
    fn a_plugin_folder_lists_its_parts_and_manifest() {
        let f = Fixture::new("plugins-parts");
        install(&f, &["p@m"]);
        let dir = "home/.claude/plugins/cache/m/p/1.0.0";
        f.write(&format!("{dir}/.claude-plugin/plugin.json"), r#"{"name":"p","version":"2.1.0","description":"Does p","agents":"./more"}"#);
        f.write(&format!("{dir}/skills/x/SKILL.md"), "---\nname: x\n---\n");
        f.write(&format!("{dir}/skills/no-skill-file/README.md"), "");
        f.write(&format!("{dir}/commands/c.md"), "");
        f.write(&format!("{dir}/commands/git/commit.md"), "");
        f.write(&format!("{dir}/agents/g.md"), "");
        f.write(&format!("{dir}/hooks/hooks.json"), r#"{"description":"d","hooks":{"Stop":[]}}"#);
        f.write(&format!("{dir}/.mcp.json"), r#"{"mcpServers":{"srv":{"command":"x","env":{"K":"sk-secret-789"}}}}"#);
        let items = f.global();
        let it = plugins(&items)[0];
        assert!(it.error.is_none(), "{:?}", it.error);
        assert_eq!(it.description, "Does p");
        let p = it.plugin.as_ref().unwrap();
        assert_eq!(p.version.as_deref(), Some("2.1.0"));
        let parts = p.parts.as_ref().unwrap();
        assert_eq!(parts.skills, vec!["x"]);
        assert_eq!(parts.commands, vec!["c", "git/commit"]);
        assert_eq!(parts.agents, vec!["g"]);
        assert_eq!(parts.hook_events, vec!["Stop"]);
        assert_eq!(parts.mcp_servers, vec!["srv"]);
        assert_eq!(parts.custom_paths, vec!["agents"]);
        assert!(!serde_json::to_string(&items).unwrap().contains("sk-secret"));
    }

    #[test]
    fn an_install_folder_outside_the_plugins_folder_is_not_read() {
        let f = Fixture::new("plugins-outside");
        let outside = f.dir.join("elsewhere/p");
        f.write("elsewhere/p/.claude-plugin/plugin.json", r#"{"description":"should not show"}"#);
        let file = serde_json::json!({ "version": 2, "plugins": { "p@m": [{ "scope": "user", "installPath": outside }] } });
        f.write("home/.claude/plugins/installed_plugins.json", &file.to_string());
        let items = f.global();
        let it = plugins(&items)[0];
        assert!(it.error.as_deref().unwrap().contains("outside"), "{:?}", it.error);
        assert!(it.description.is_empty() && it.plugin.as_ref().unwrap().parts.is_none());
    }

    #[test]
    fn an_unreadable_settings_file_leaves_the_plugin_state_unknown() {
        let f = Fixture::new("plugins-unknown");
        install(&f, &["a@m"]);
        f.write("home/.claude/settings.json", "{");
        let items = f.global();
        let it = plugins(&items).into_iter().find(|i| i.plugin.is_some()).unwrap();
        assert_eq!(it.plugin.as_ref().unwrap().enabled, None);
        assert!(it.error.as_deref().unwrap().contains("unknown"), "{:?}", it.error);
    }

    #[test]
    fn a_project_value_is_unknown_when_its_local_file_is_unreadable() {
        let f = Fixture::new("plugins-local-unknown");
        f.write("proj/.claude/settings.json", r#"{"enabledPlugins":{"a@m":true}}"#);
        f.write("proj/.claude/settings.local.json", "{");
        let items = f.project();
        let it = plugins(&items).into_iter().find(|i| i.plugin.is_some()).unwrap();
        assert_eq!(it.plugin.as_ref().unwrap().enabled, None);
        assert!(it.error.as_deref().unwrap().contains("unknown"), "{:?}", it.error);
    }

    #[test]
    fn a_malformed_local_enabled_plugins_leaves_the_project_value_unknown() {
        let f = Fixture::new("plugins-local-malformed");
        f.write("proj/.claude/settings.json", r#"{"enabledPlugins":{"a@m":true}}"#);
        f.write("proj/.claude/settings.local.json", r#"{"enabledPlugins":["a@m"]}"#);
        let items = f.project();
        let it = plugins(&items).into_iter().find(|i| i.plugin.is_some()).unwrap();
        assert_eq!(it.plugin.as_ref().unwrap().enabled, None);
        assert_eq!(it.source, ConfigSource::Local);
        assert!(it.error.as_deref().unwrap().contains("unknown"), "{:?}", it.error);
    }

    #[test]
    fn the_user_install_is_preferred_over_a_project_one() {
        let f = Fixture::new("plugins-user-install");
        let user = f.home().join(".claude/plugins/cache/m/a/2.0.0");
        fs::create_dir_all(&user).unwrap();
        let file = serde_json::json!({ "version": 2, "plugins": { "a@m": [
            { "scope": "project", "installPath": "/nowhere/project-install", "version": "1.0.0" },
            { "scope": "user", "installPath": user, "version": "2.0.0" },
        ] } });
        f.write("home/.claude/plugins/installed_plugins.json", &file.to_string());
        let items = f.global();
        let p = plugins(&items)[0].plugin.as_ref().unwrap();
        assert_eq!((p.install_scope.as_deref(), p.version.as_deref()), (Some("user"), Some("2.0.0")));
    }

    #[test]
    fn an_enabled_value_that_is_not_a_boolean_is_an_error_on_its_row() {
        let f = Fixture::new("plugins-nonbool");
        f.write("home/.claude/settings.json", r#"{"enabledPlugins":{"a@m":"yes"}}"#);
        let items = f.global();
        let it = plugins(&items)[0];
        assert_eq!(it.plugin.as_ref().unwrap().enabled, None);
        assert!(it.error.as_deref().unwrap().contains("not true or false"), "{:?}", it.error);
    }
}
