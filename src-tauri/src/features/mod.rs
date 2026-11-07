//! Feature toggles: the module → prop registry (report §4.2–4.3) and its pure
//! resolution.
//!
//! - [`MODULES`]: the one declaration of every module and prop — fresh-install
//!   default, lock, dependencies, legacy key. The frontend keeps no defaults.
//! - [`resolve`]: stored `settings` rows → the effective tree (no DB access).
//! - `commands`: the Tauri commands (`get_features`, `set_feature`,
//!   `reset_features`); [`apply`]: what runs after a switch is written;
//!   [`seed`]: the one-time startup seed (fresh install vs upgrade).
//!
//! Storage: rows `feature.<module>` / `feature.<module>.<prop>` of the
//! `settings` table, or the prop's legacy key when it has one. A prop id may
//! itself contain dots (`selection.translate`), so a key is mapped back to its
//! entry by matching the registry ([`owner`]), never by splitting on `.`.
//! Values follow the existing switches: only an explicit `"false"` is off.

pub mod apply;
pub mod commands;
pub mod seed;

use serde::Serialize;
use std::collections::HashMap;

pub const PREFIX: &str = "feature.";

pub struct Module {
    pub id: &'static str,
    /// Fresh-install value (an upgrade keeps everything on: seeded at startup).
    pub default: bool,
    /// Core: shown but always on.
    pub locked: bool,
    pub props: &'static [Prop],
}

pub struct Prop {
    /// Local to its module; may contain dots.
    pub id: &'static str,
    pub default: bool,
    pub locked: bool,
    /// Full ids (`module.prop`) this prop works best with. A missing one does
    /// not turn the prop off; it runs degraded.
    pub depends_on: &'static [&'static str],
    /// Pre-registry `settings` key the prop reads and writes instead of
    /// `feature.<module>.<prop>`.
    pub legacy_key: Option<&'static str>,
}

const fn on(id: &'static str) -> Prop {
    Prop { id, default: true, locked: false, depends_on: &[], legacy_key: None }
}

const fn off(id: &'static str) -> Prop {
    Prop { id, default: false, locked: false, depends_on: &[], legacy_key: None }
}

const fn core(id: &'static str) -> Prop {
    Prop { id, default: true, locked: true, depends_on: &[], legacy_key: None }
}

/// Only props that already have code; a new feature registers its prop in the
/// task that builds it.
pub static MODULES: &[Module] = &[
    Module {
        id: "sessions",
        default: true,
        locked: true,
        props: &[core("core"), on("handoff"), on("mirror_claude"), on("mirror_codex")],
    },
    Module {
        id: "text_tools",
        default: true,
        locked: false,
        props: &[
            on("selection.translate"),
            Prop { depends_on: &["productivity.notes"], ..on("selection.save_note") },
            Prop { depends_on: &["productivity.todos"], ..on("selection.save_todo") },
        ],
    },
    Module {
        id: "library",
        default: true,
        locked: true,
        props: &[
            core("skills"),
            core("rules"),
            core("mcp"),
            on("groups"),
            on("instruction_checks"),
            Prop { legacy_key: Some("mcp_builtin_puppeteer_enabled"), ..on("builtin_mcp_puppeteer") },
        ],
    },
    Module { id: "orchestration", default: true, locked: false, props: &[on("conductor")] },
    Module {
        id: "code_hosting",
        default: true,
        locked: false,
        props: &[core("issue_pr_runs"), on("pr_inbox"), off("gantt")],
    },
    Module {
        id: "productivity",
        default: true,
        locked: false,
        props: &[
            on("notes"),
            off("todos"),
            // Capacity (focus::capacity) needs the calendar.
            Prop { depends_on: &["productivity.todos", "productivity.calendar"], ..off("focus") },
            Prop { depends_on: &["integrations.google"], ..off("calendar") },
        ],
    },
    Module {
        id: "integrations",
        default: true,
        locked: false,
        props: &[
            // The `google` MCP also needs `mcp_builtin_google_enabled` (MCP page
            // switch); pointing the prop at it would drop the account with the MCP.
            off("google"),
            off("slack"),
            // One source of truth for the listener: the switch it already reads.
            Prop { legacy_key: Some(crate::inbox_api::ENABLED_SETTING_KEY), ..off("inbox_api") },
        ],
    },
    Module {
        id: "infra_access",
        default: true,
        locked: false,
        props: &[off("vps"), off("aws"), off("azure")],
    },
    Module {
        id: "remote",
        default: false,
        locked: false,
        props: &[off("remote_control"), on("web_push"), on("remote_desktop"), on("file_transfer")],
    },
    Module { id: "security", default: true, locked: false, props: &[off("secret_vault")] },
    Module {
        id: "usage",
        default: true,
        locked: false,
        props: &[on("stats"), on("context_meter"), on("budget_guard")],
    },
    Module {
        id: "app",
        default: true,
        locked: false,
        props: &[Prop { legacy_key: Some("animated_background"), ..on("animated_background") }],
    },
];

impl Module {
    pub fn key(&self) -> String {
        format!("{PREFIX}{}", self.id)
    }
}

impl Prop {
    pub fn key(&self, module: &Module) -> String {
        match self.legacy_key {
            Some(k) => k.to_string(),
            None => format!("{PREFIX}{}.{}", module.id, self.id),
        }
    }
}

/// The registry entry stored under `key`: a module (prop `None`) or one of its
/// props. `None` for any key the registry does not own.
pub fn owner(key: &str) -> Option<(&'static Module, Option<&'static Prop>)> {
    MODULES.iter().find_map(|m| {
        if key == m.key() {
            return Some((m, None));
        }
        m.props.iter().find(|p| key == p.key(m)).map(|p| (m, Some(p)))
    })
}

/// The registry entry of a full id: a module (`productivity`, prop `None`) or
/// one of its props (`text_tools.selection.translate`).
pub fn entry(id: &str) -> Option<(&'static Module, Option<&'static Prop>)> {
    MODULES.iter().find_map(|m| {
        if id == m.id {
            return Some((m, None));
        }
        let prop = id.strip_prefix(m.id)?.strip_prefix('.')?;
        m.props.iter().find(|p| p.id == prop).map(|p| (m, Some(p)))
    })
}

/// The fresh-install value of every switch that can be turned off, as the rows
/// `reset_features` writes. Legacy keys get an explicit value too: a missing
/// `inbox_api_enabled` means on to the Inbox API.
pub fn default_rows() -> HashMap<String, String> {
    MODULES
        .iter()
        .flat_map(|m| {
            let module = (!m.locked).then(|| (m.key(), m.default));
            let props = m.props.iter().filter(|p| !p.locked).map(move |p| (p.key(m), p.default));
            module.into_iter().chain(props)
        })
        .map(|(k, v)| (k, v.to_string()))
        .collect()
}

/// Full ids that are on in `before` and off in `after`: modules, then props.
pub fn turned_off(before: &[ModuleState], after: &[ModuleState]) -> Vec<String> {
    let modules = before
        .iter()
        .zip(after)
        .filter(|(b, a)| b.enabled && !a.enabled)
        .map(|(_, a)| a.module.to_string());
    let props = before
        .iter()
        .zip(after)
        .flat_map(|(b, a)| b.props.iter().zip(&a.props))
        .filter(|(b, a)| b.effective && !a.effective)
        .map(|(_, a)| a.id.clone());
    modules.chain(props).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleState {
    pub module: &'static str,
    pub enabled: bool,
    pub locked: bool,
    pub props: Vec<PropState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PropState {
    /// Full id, `module.prop`.
    pub id: String,
    /// Saved value; `None` = never saved (the default applies).
    pub stored: Option<bool>,
    pub effective: bool,
    pub locked: bool,
    pub default: bool,
    /// Full ids of dependencies that are off, while this prop is on.
    pub degraded: Vec<String>,
    /// i18n key explaining a locked, module-off or degraded prop.
    pub reason_key: Option<&'static str>,
}

fn stored(rows: &HashMap<String, String>, key: &str) -> Option<bool> {
    rows.get(key).map(|v| v.trim() != "false")
}

/// Effective tree from the `settings` rows (rows the registry does not own are
/// ignored). A module is on when locked or stored/default on; a prop is on
/// when locked, or its module and itself are on. Turning a module off never
/// touches its props' stored values, so turning it back on restores them.
pub fn resolve(rows: &HashMap<String, String>) -> Vec<ModuleState> {
    let mut tree: Vec<ModuleState> = MODULES
        .iter()
        .map(|m| {
            let enabled = m.locked || stored(rows, &m.key()).unwrap_or(m.default);
            let props = m
                .props
                .iter()
                .map(|p| {
                    let stored = stored(rows, &p.key(m));
                    let effective = p.locked || (enabled && stored.unwrap_or(p.default));
                    PropState {
                        id: format!("{}.{}", m.id, p.id),
                        stored,
                        effective,
                        locked: p.locked,
                        default: p.default,
                        degraded: Vec::new(),
                        reason_key: None,
                    }
                })
                .collect();
            ModuleState { module: m.id, enabled, locked: m.locked, props }
        })
        .collect();

    // Second pass: dependencies are checked against effective values.
    let effective: HashMap<String, bool> = tree
        .iter()
        .flat_map(|m| m.props.iter().map(|p| (p.id.clone(), p.effective)))
        .collect();
    for (m, module) in tree.iter_mut().zip(MODULES) {
        for (p, prop) in m.props.iter_mut().zip(module.props) {
            if p.effective {
                p.degraded = prop
                    .depends_on
                    .iter()
                    .filter(|d| !effective.get(**d).copied().unwrap_or(false))
                    .map(|d| d.to_string())
                    .collect();
            }
            p.reason_key = if p.locked {
                Some("features.reason.locked")
            } else if !m.enabled {
                Some("features.reason.module_off")
            } else if !p.degraded.is_empty() {
                Some("features.reason.missing_dependency")
            } else {
                None
            };
        }
    }
    tree
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn rows(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn prop<'a>(tree: &'a [ModuleState], id: &str) -> &'a PropState {
        tree.iter().flat_map(|m| &m.props).find(|p| p.id == id).unwrap()
    }

    fn module<'a>(tree: &'a [ModuleState], id: &str) -> &'a ModuleState {
        tree.iter().find(|m| m.module == id).unwrap()
    }

    #[test]
    fn fresh_install_defaults_follow_the_table() {
        let tree = resolve(&HashMap::new());
        assert!(!module(&tree, "remote").enabled);
        assert!(!prop(&tree, "remote.web_push").effective, "on in a module that is off");
        assert!(prop(&tree, "remote.web_push").default);
        assert!(prop(&tree, "productivity.notes").effective);
        assert!(!prop(&tree, "productivity.todos").effective);
        assert!(!prop(&tree, "integrations.inbox_api").effective);
        assert!(prop(&tree, "library.builtin_mcp_puppeteer").effective);
        assert!(prop(&tree, "app.animated_background").effective);
        assert!(tree.iter().flat_map(|m| &m.props).all(|p| p.stored.is_none()));
    }

    // The three examples of report §4.2.
    #[test]
    fn module_switch_keeps_prop_choices() {
        let mut r = rows(&[
            ("feature.productivity", "true"),
            ("feature.productivity.focus", "false"),
            ("feature.productivity.notes", "true"),
        ]);
        let tree = resolve(&r);
        assert!(prop(&tree, "productivity.notes").effective);
        assert!(!prop(&tree, "productivity.focus").effective);

        r.insert("feature.productivity".into(), "false".into());
        let tree = resolve(&r);
        assert!(!module(&tree, "productivity").enabled);
        assert!(!prop(&tree, "productivity.notes").effective);
        assert!(!prop(&tree, "productivity.focus").effective);
        assert_eq!(prop(&tree, "productivity.notes").stored, Some(true));
        assert_eq!(prop(&tree, "productivity.notes").reason_key, Some("features.reason.module_off"));

        r.insert("feature.productivity".into(), "true".into());
        let tree = resolve(&r);
        assert!(prop(&tree, "productivity.notes").effective);
        assert!(!prop(&tree, "productivity.focus").effective);
    }

    #[test]
    fn locked_entries_stay_on() {
        let tree = resolve(&rows(&[
            ("feature.sessions", "false"),
            ("feature.sessions.core", "false"),
            ("feature.code_hosting", "false"),
            ("feature.code_hosting.issue_pr_runs", "false"),
        ]));
        assert!(module(&tree, "sessions").enabled);
        let core = prop(&tree, "sessions.core");
        assert!(core.effective && core.locked);
        assert_eq!(core.stored, Some(false));
        assert_eq!(core.reason_key, Some("features.reason.locked"));
        // A core prop inside a module that can be turned off.
        assert!(!module(&tree, "code_hosting").enabled);
        assert!(prop(&tree, "code_hosting.issue_pr_runs").effective);
        assert!(!prop(&tree, "code_hosting.pr_inbox").effective);
    }

    #[test]
    fn missing_dependency_degrades_but_keeps_the_prop_on() {
        let tree = resolve(&rows(&[
            ("feature.productivity.todos", "true"),
            ("feature.productivity.focus", "true"),
        ]));
        let focus = prop(&tree, "productivity.focus");
        assert!(focus.effective);
        assert_eq!(focus.degraded, vec!["productivity.calendar".to_string()]);
        assert_eq!(focus.reason_key, Some("features.reason.missing_dependency"));
        // Off props report no degradation.
        assert!(prop(&tree, "productivity.calendar").degraded.is_empty());

        let tree = resolve(&rows(&[
            ("feature.productivity.todos", "true"),
            ("feature.productivity.focus", "true"),
            ("feature.productivity.calendar", "true"),
            ("feature.integrations.google", "true"),
        ]));
        assert!(prop(&tree, "productivity.focus").degraded.is_empty());
        assert_eq!(prop(&tree, "productivity.focus").reason_key, None);
    }

    #[test]
    fn legacy_key_is_read_instead_of_the_feature_key() {
        let tree = resolve(&rows(&[
            ("inbox_api_enabled", "true"),
            ("feature.integrations.inbox_api", "false"),
            ("animated_background", "false"),
            ("feature.app.animated_background", "true"),
            ("mcp_builtin_puppeteer_enabled", "false"),
        ]));
        let inbox = prop(&tree, "integrations.inbox_api");
        assert!(inbox.effective);
        assert_eq!(inbox.stored, Some(true));
        assert!(!prop(&tree, "app.animated_background").effective);
        assert!(!prop(&tree, "library.builtin_mcp_puppeteer").effective);
        assert!(owner("feature.integrations.inbox_api").is_none());
        // `mcp_builtin_google_enabled` stays the MCP page's switch, not the prop's.
        assert!(owner("mcp_builtin_google_enabled").is_none());
        let (m, p) = owner("feature.integrations.google").unwrap();
        assert_eq!((m.id, p.unwrap().id), ("integrations", "google"));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let r = rows(&[
            ("feature.nope", "false"),
            ("feature.sessions.nope", "false"),
            ("feature._seeded", "1"),
            ("feature.text_tools.selection", "false"),
            ("theme", "dark"),
        ]);
        assert_eq!(resolve(&r), resolve(&HashMap::new()));
        for k in r.keys() {
            assert!(owner(k).is_none(), "{k}");
        }
    }

    #[test]
    fn dotted_prop_keys_map_back_to_their_entry() {
        let (m, p) = owner("feature.text_tools.selection.translate").unwrap();
        assert_eq!((m.id, p.unwrap().id), ("text_tools", "selection.translate"));
        let (m, p) = owner("feature.text_tools").unwrap();
        assert_eq!(m.id, "text_tools");
        assert!(p.is_none());
        let (m, p) = owner("inbox_api_enabled").unwrap();
        assert_eq!((m.id, p.unwrap().id), ("integrations", "inbox_api"));

        let tree = resolve(&rows(&[("feature.text_tools.selection.translate", "false")]));
        assert!(!prop(&tree, "text_tools.selection.translate").effective);
        assert!(prop(&tree, "text_tools.selection.save_note").default);
    }

    #[test]
    fn registry_is_consistent() {
        let mut keys = HashSet::new();
        let mut ids = HashSet::new();
        for m in MODULES {
            assert!(keys.insert(m.key()), "duplicate key {}", m.key());
            for p in m.props {
                assert!(keys.insert(p.key(m)), "duplicate key {}", p.key(m));
                assert!(ids.insert(format!("{}.{}", m.id, p.id)));
                assert!(!p.locked || p.default, "locked prop {} must default on", p.id);
            }
            assert!(!m.locked || m.default, "locked module {} must default on", m.id);
        }
        for m in MODULES {
            for p in m.props {
                for d in p.depends_on {
                    assert!(ids.contains(*d), "{}.{} depends on unknown {d}", m.id, p.id);
                }
            }
        }
        // Every key round-trips through `owner`.
        for k in &keys {
            assert!(owner(k).is_some(), "{k}");
        }
    }

    #[test]
    fn entry_finds_modules_and_dotted_props() {
        let (m, p) = entry("productivity").unwrap();
        assert_eq!(m.id, "productivity");
        assert!(p.is_none());
        let (m, p) = entry("text_tools.selection.translate").unwrap();
        assert_eq!((m.id, p.unwrap().id), ("text_tools", "selection.translate"));
        for id in ["", "nope", "text_tools.selection", "productivity.", "remote_control", "sessions.nope"] {
            assert!(entry(id).is_none(), "{id}");
        }
    }

    #[test]
    fn default_rows_resolve_to_a_fresh_install() {
        let defaults = default_rows();
        let (reset, fresh) = (resolve(&defaults), resolve(&HashMap::new()));
        // Same switches; only `stored` differs.
        assert!(turned_off(&reset, &fresh).is_empty() && turned_off(&fresh, &reset).is_empty());
        assert_eq!(defaults.get("inbox_api_enabled").map(String::as_str), Some("false"));
        assert_eq!(defaults.get("animated_background").map(String::as_str), Some("true"));
        assert_eq!(defaults.get("feature.remote").map(String::as_str), Some("false"));
        assert!(!defaults.contains_key("feature.sessions"), "locked module");
        assert!(!defaults.contains_key("feature.code_hosting.issue_pr_runs"), "locked prop");
        assert!(defaults.keys().all(|k| owner(k).is_some()));
    }

    #[test]
    fn turned_off_lists_modules_and_props_that_go_off() {
        let before = resolve(&rows(&[("feature.remote", "true"), ("feature.productivity.todos", "true")]));
        let after = resolve(&HashMap::new());
        assert_eq!(
            turned_off(&before, &after),
            ["remote", "productivity.todos", "remote.web_push", "remote.remote_desktop", "remote.file_transfer"]
        );
        assert!(turned_off(&after, &before).is_empty());
    }
}
