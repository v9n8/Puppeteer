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
use crate::remote::guard::{FileId, SecretPaths, SECRET_HOME_FILES, SECRET_HOME_FILE_PREFIXES};
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
    /// Empty on the notice row of a file that sets `disableAllHooks` and lists no hook (its id
    /// starts `hook-off:`, which is what tells it apart).
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

/// Whether `real` (canonical) is one of the guard's credential files in HOME (`real_home`,
/// canonical): by its own path, case-insensitively as the guard compares, or as the same file as
/// one that is itself a link (a dotfiles file). Claude Code's own settings files don't count
/// there: a settings link leads to that same file by definition, and both may share one target.
fn credential_file_at(real_home: &Path, real: &Path) -> bool {
    let Ok(rel) = real.strip_prefix(real_home) else { return false };
    let rel: Vec<String> = rel.iter().map(|c| c.to_string_lossy().to_lowercase()).collect();
    let by_path = SECRET_HOME_FILES.iter().any(|f| f.split('/').eq(rel.iter().map(String::as_str)))
        || (rel.len() == 1 && SECRET_HOME_FILE_PREFIXES.iter().any(|pre| rel[0].starts_with(pre)));
    if by_path {
        return true;
    }
    let Some(id) = FileId::of(real) else { return false };
    let prefixed = fs::read_dir(real_home)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| SECRET_HOME_FILE_PREFIXES.iter().any(|pre| n.to_lowercase().starts_with(pre)));
    SECRET_HOME_FILES
        .iter()
        .filter(|f| !f.starts_with(".claude/settings"))
        .map(|f| f.to_string())
        .chain(prefixed)
        .any(|f| FileId::of(&f.split('/').fold(real_home.to_path_buf(), |p, c| p.join(c))) == Some(id))
}

/// The JSON object at `rel` under `base`; `None` when there is no file.
fn read_json(base: &Path, rel: &str, reach: Reach, secret: &SecretPaths) -> Result<Option<Map<String, Value>>, String> {
    let at = base.join(rel);
    if fs::symlink_metadata(&at).is_err() {
        return Ok(None);
    }
    let real_base = fs::canonicalize(base).map_err(|_| format!("Folder not found: {}", base.display()))?;
    let real = fs::canonicalize(&at).map_err(|_| format!("{} is a link to something that doesn't exist", at.display()))?;
    if !folder_allowed(&real, &real_base, Path::new(rel), secret) {
        let in_home = reach == Reach::Home
            && real.starts_with(&real_base)
            && real.parent().is_some_and(|dir| !secret.denies(dir));
        if !in_home {
            return Err(format!("{} leads outside {} — Puppeteer won't open it", at.display(), base.display()));
        }
        if credential_file_at(&real_base, &real) {
            return Err(format!("{} leads to a credential file — Puppeteer won't open it", at.display()));
        }
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

/// The hook rows of one settings file; a notice row when it turns every hook off and lists none,
/// which would otherwise not show.
fn hooks_of(settings: &Map<String, Value>, source: ConfigSource, file: &Path, out: &mut Vec<ConfigItem>) {
    let all_disabled = settings.get("disableAllHooks").and_then(Value::as_bool) == Some(true);
    let before = out.len();
    if let Some(hooks) = settings.get("hooks") {
        listed_hooks(hooks, all_disabled, source, file, out);
    }
    if all_disabled && out.len() == before {
        out.push(ConfigItem {
            kind: ConfigKind::Hook,
            id: format!("hook-off:{}", source.as_str()),
            name: "disableAllHooks".to_string(),
            description: String::new(),
            source,
            file: file.display().to_string(),
            error: None,
            hook: Some(HookInfo {
                event: String::new(),
                matcher: None,
                hook_type: String::new(),
                command: String::new(),
                timeout: None,
                all_disabled,
            }),
            plugin: None,
        });
    }
}

/// One row per hook entry of each event's matcher groups.
fn listed_hooks(hooks: &Value, all_disabled: bool, source: ConfigSource, file: &Path, out: &mut Vec<ConfigItem>) {
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
mod tests;
