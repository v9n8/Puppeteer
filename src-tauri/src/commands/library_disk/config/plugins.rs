//! The Plugins tab: what `~/.claude/plugins/installed_plugins.json` lists and what each level's
//! `enabledPlugins` sets. Global lists both; a project lists only the plugins its own two files
//! set, its details taken from HOME's install by key. A plugin's folder is read only inside
//! `~/.claude/plugins/`, and only for names: its MCP servers are managed under MCP.

use super::{error_item, read_json, ConfigItem, ConfigKind, ConfigSource, Reach};
use crate::remote::guard::SecretPaths;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const PLUGINS_REL: &str = ".claude/plugins";
const INSTALLED_REL: &str = ".claude/plugins/installed_plugins.json";
/// `plugin.json` keys that point the CLI at other paths than the default folders.
const CUSTOM_PATH_KEYS: &[&str] = &["commands", "agents", "skills", "hooks", "mcpServers", "lspServers", "outputStyles"];

#[derive(Debug, Clone, Default, Serialize)]
pub struct PluginParts {
    pub skills: Vec<String>,
    pub commands: Vec<String>,
    pub agents: Vec<String>,
    pub hook_events: Vec<String>,
    /// Names only: MCP servers are managed under MCP.
    pub mcp_servers: Vec<String>,
    /// `plugin.json` keys naming other paths; not followed, so the lists above may miss parts.
    pub custom_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    /// `name@marketplace`, as `enabledPlugins` keys it.
    pub key: String,
    pub marketplace: Option<String>,
    /// The value the most specific file read sets; none when no file sets it.
    pub enabled: Option<bool>,
    /// A file that could set it couldn't be read, or set something else than true / false:
    /// `enabled` none means unknown, not "not set".
    pub unknown: bool,
    pub installed: bool,
    pub version: Option<String>,
    pub install_path: Option<String>,
    /// `user`, `project`, … as `installed_plugins.json` writes it.
    pub install_scope: Option<String>,
    pub parts: Option<PluginParts>,
}

/// One level's `enabledPlugins`: key → its value, `None` when that is not a boolean.
pub(super) type Enabled = BTreeMap<String, Option<bool>>;

/// A settings file that was read, with what it enables.
pub(super) struct Level {
    pub source: ConfigSource,
    pub file: PathBuf,
    pub enabled: Enabled,
}

/// A settings file whose `enabledPlugins` couldn't be read (the file, or that key): what it sets
/// is unknown.
pub(super) struct Unread {
    pub source: ConfigSource,
    pub file: PathBuf,
}

/// Higher wins: local over project over user.
fn rank(source: ConfigSource) -> u8 {
    match source {
        ConfigSource::User => 0,
        ConfigSource::Project => 1,
        ConfigSource::Local => 2,
    }
}

pub(super) fn enabled_of(settings: &Map<String, Value>, file: &Path) -> Result<Enabled, String> {
    match settings.get("enabledPlugins") {
        None => Ok(Enabled::new()),
        Some(Value::Object(map)) => Ok(map.iter().map(|(k, v)| (k.clone(), v.as_bool())).collect()),
        Some(_) => Err(format!("\"enabledPlugins\" in {} is not an object", file.display())),
    }
}

/// One install of a plugin, as `installed_plugins.json` lists it.
struct Install {
    path: Option<String>,
    scope: Option<String>,
    version: Option<String>,
}

/// Each key's install: the user-scope one when there is one (a project's install belongs to that
/// project), else the first. Version 2 keeps a list per key; a single object is taken as is.
fn installs_of(file: &Map<String, Value>) -> BTreeMap<String, Install> {
    let str_of = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let Some(plugins) = file.get("plugins").and_then(Value::as_object) else { return BTreeMap::new() };
    plugins
        .iter()
        .filter_map(|(key, v)| {
            let first = match v {
                Value::Array(list) => list
                    .iter()
                    .find(|i| i.get("scope").and_then(Value::as_str) == Some("user"))
                    .or_else(|| list.first())?,
                other => other,
            };
            first.is_object().then(|| {
                (key.clone(), Install {
                    path: str_of(first, "installPath"),
                    scope: str_of(first, "scope"),
                    version: str_of(first, "version"),
                })
            })
        })
        .collect()
}

/// What a plugin's folder holds, with its manifest's description and version.
struct Folder {
    description: Option<String>,
    version: Option<String>,
    parts: PluginParts,
}

/// Names of the entries of `dir` (inside `root`) that `pick` keeps, sorted.
fn names(root: &Path, dir: &str, pick: impl Fn(&Path, &str) -> Option<String>) -> Vec<String> {
    let at = root.join(dir);
    let Ok(real) = fs::canonicalize(&at) else { return Vec::new() };
    if !real.starts_with(root) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(&real) else { return Vec::new() };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            pick(&e.path(), &name)
        })
        .collect();
    out.sort();
    out
}

fn md_name(path: &Path, name: &str) -> Option<String> {
    name.strip_suffix(".md").filter(|n| !n.is_empty() && path.is_file()).map(str::to_string)
}

/// `commands/*.md`, and one folder down as `folder/name` (the CLI namespaces those).
fn command_names(root: &Path) -> Vec<String> {
    let folders = names(root, "commands", |p, n| p.is_dir().then(|| n.to_string()));
    let mut out = names(root, "commands", md_name);
    for folder in folders {
        out.extend(names(root, &format!("commands/{folder}"), md_name).into_iter().map(|c| format!("{folder}/{c}")));
    }
    out.sort();
    out
}

/// Keys of the object at `key` of `file`, else of `file` itself when `key` is absent.
fn keys_of(file: Option<Map<String, Value>>, key: &str) -> Vec<String> {
    let Some(file) = file else { return Vec::new() };
    match file.get(key) {
        Some(Value::Object(inner)) => inner.keys().cloned().collect(),
        Some(_) => Vec::new(),
        None => file.keys().cloned().collect(),
    }
}

fn folder_of(home: &Path, install_path: &Path, secret: &SecretPaths) -> Result<Folder, String> {
    let plugins = home.join(PLUGINS_REL);
    let refused = || format!("{} is outside {} — Puppeteer won't read it", install_path.display(), plugins.display());
    let root = fs::canonicalize(&plugins).map_err(|_| format!("Folder not found: {}", plugins.display()))?;
    let real = fs::canonicalize(install_path).map_err(|_| format!("Install folder not found: {}", install_path.display()))?;
    if !real.starts_with(&root) || secret.denies(&real) {
        return Err(refused());
    }
    if !real.is_dir() {
        return Err(format!("{} is not a folder", install_path.display()));
    }
    let manifest = read_json(&real, ".claude-plugin/plugin.json", Reach::Library, secret)?;
    let manifest_str = |key: &str| manifest.as_ref()?.get(key)?.as_str().map(str::to_string);
    let mut hook_events = keys_of(read_json(&real, "hooks/hooks.json", Reach::Library, secret)?, "hooks");
    hook_events.retain(|k| k != "description");
    let parts = PluginParts {
        skills: names(&real, "skills", |p, n| p.join("SKILL.md").is_file().then(|| n.to_string())),
        commands: command_names(&real),
        agents: names(&real, "agents", md_name),
        hook_events,
        mcp_servers: keys_of(read_json(&real, ".mcp.json", Reach::Library, secret)?, "mcpServers"),
        custom_paths: CUSTOM_PATH_KEYS
            .iter()
            .filter(|k| manifest.as_ref().is_some_and(|m| m.contains_key(**k)))
            .map(|k| k.to_string())
            .collect(),
    };
    Ok(Folder { description: manifest_str("description"), version: manifest_str("version"), parts })
}

/// The plugin rows. Global (`global`): every installed plugin and every one the user's file
/// sets. A project: every plugin its files set, the most specific file's value counting. A value
/// an `unread` file more specific than the one that set it (or any, when none did) might override
/// is unknown.
pub(super) fn plugins_of(
    home: &Path,
    global: bool,
    levels: &[Level],
    unread: &[Unread],
    secret: &SecretPaths,
    out: &mut Vec<ConfigItem>,
) {
    let installed_file = home.join(INSTALLED_REL);
    // Not a credential file: the Library's rule follows a dotfiles link to it already.
    let installed = match read_json(home, INSTALLED_REL, Reach::Library, secret) {
        Ok(Some(file)) => installs_of(&file),
        Ok(None) => BTreeMap::new(),
        Err(e) => {
            out.push(error_item(ConfigKind::Plugin, ConfigSource::User, &installed_file, 0, e));
            BTreeMap::new()
        }
    };
    let mut keys: BTreeSet<&String> = levels.iter().flat_map(|l| l.enabled.keys()).collect();
    if global {
        keys.extend(installed.keys());
    }
    for key in keys {
        // Levels run from the least to the most specific (project, then local).
        let set_by = levels.iter().rev().find_map(|l| l.enabled.get(key).map(|v| (l, *v)));
        let install = installed.get(key);
        let (name, marketplace) = match key.split_once('@') {
            Some((n, m)) => (n.to_string(), Some(m.to_string())),
            None => (key.clone(), None),
        };
        let hidden_by = unread.iter().find(|u| set_by.is_none_or(|(l, _)| rank(u.source) > rank(l.source)));
        // The file that decides the state: the unreadable one when it may.
        let (source, file) = match (hidden_by, set_by) {
            (Some(u), _) => (u.source, u.file.clone()),
            (None, Some((l, _))) => (l.source, l.file.clone()),
            (None, None) => (ConfigSource::User, installed_file.clone()),
        };
        let mut error = match (hidden_by, set_by) {
            (Some(u), _) => Some(format!(
                "enabledPlugins in {} can't be read (see its error row), so whether this plugin is on is unknown",
                u.file.display()
            )),
            (None, Some((l, None))) => Some(format!("\"enabledPlugins.{key}\" in {} is not true or false", l.file.display())),
            _ => None,
        };
        let mut info = PluginInfo {
            key: key.clone(),
            marketplace,
            enabled: if hidden_by.is_some() { None } else { set_by.and_then(|(_, v)| v) },
            unknown: hidden_by.is_some() || matches!(set_by, Some((_, None))),
            installed: install.is_some(),
            version: install.and_then(|i| i.version.clone()),
            install_path: install.and_then(|i| i.path.clone()),
            install_scope: install.and_then(|i| i.scope.clone()),
            parts: None,
        };
        let mut description = String::new();
        if let Some(path) = &info.install_path {
            match folder_of(home, Path::new(path), secret) {
                Ok(folder) => {
                    description = folder.description.unwrap_or_default();
                    info.version = folder.version.or(info.version);
                    info.parts = Some(folder.parts);
                }
                Err(e) => error = error.or(Some(e)),
            }
        }
        out.push(ConfigItem {
            kind: ConfigKind::Plugin,
            id: format!("plugin:{key}"),
            name,
            description,
            source,
            file: file.display().to_string(),
            error,
            hook: None,
            plugin: Some(info),
        });
    }
}
