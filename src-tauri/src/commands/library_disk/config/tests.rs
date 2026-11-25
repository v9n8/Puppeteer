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
fn a_file_that_only_turns_hooks_off_gets_a_notice_row() {
    let f = Fixture::new("off-notice");
    f.write("proj/.claude/settings.local.json", r#"{"disableAllHooks":true}"#);
    let items = f.project();
    assert_eq!(items.len(), 1, "{items:?}");
    let hook = items[0].hook.as_ref().unwrap();
    assert!(hook.all_disabled && hook.event.is_empty());
    assert_eq!((items[0].name.as_str(), items[0].source), ("disableAllHooks", ConfigSource::Local));
    // With hooks listed, their rows carry it instead; off, nothing.
    f.write("proj/.claude/settings.local.json", r#"{"disableAllHooks":true,"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"}]}]}}"#);
    assert_eq!(f.project().len(), 1);
    assert!(!f.project()[0].hook.as_ref().unwrap().event.is_empty());
    f.write("proj/.claude/settings.local.json", r#"{"disableAllHooks":false}"#);
    assert!(f.project().is_empty());
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

#[cfg(unix)]
#[test]
fn a_user_settings_link_to_a_linked_credential_file_is_not_read() {
    let f = Fixture::new("dotfiles-linked-credential");
    // Both the credential file and the settings file are dotfiles links to the same file.
    let target = f.write("home/dotfiles/npmrc", PRE_BASH);
    std::os::unix::fs::symlink(&target, f.home().join(".npmrc")).unwrap();
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
    // A refused folder says nothing about the state: no file sets it, so it is not set.
    assert!(!it.plugin.as_ref().unwrap().unknown);
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
    assert!(it.plugin.as_ref().unwrap().unknown);
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
    assert!(it.plugin.as_ref().unwrap().unknown);
    assert!(it.error.as_deref().unwrap().contains("not true or false"), "{:?}", it.error);
}
