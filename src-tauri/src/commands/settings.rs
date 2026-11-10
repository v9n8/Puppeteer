use crate::db::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::State;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppSettings {
    pub default_engine: String,
    pub claude_path: String,
    pub codex_path: String,
    /// Default model for Claude runs (SDK alias: ""=engine default, opus, sonnet, haiku).
    pub claude_model: String,
    /// Default model for Codex runs ("" = engine default, e.g. gpt-5-codex, gpt-5).
    pub codex_model: String,
    pub extra_args: String,
    pub theme: String,
    /// UI language: "en" (default) | "vi".
    pub language: String,
    /// Named color palette: "default" or an id from `src/lib/colorThemes.ts`.
    pub color_theme: String,
    /// Enable the animated decorative background for scenic themes (e.g. the
    /// Full Moon night scene). "true" (default) | "false".
    pub animated_background: String,
    /// User's reusable prompt library, stored as a JSON array of
    /// `{ id, title, body }`. Defaults to `"[]"`. The UI owns the shape; the
    /// backend only round-trips the string.
    pub saved_prompts: String,
    pub default_permission_mode: String,
    /// Default reasoning effort for new runs: "low" | "medium" | "high" | "xhigh" |
    /// "max", or "" to let the engine decide. A run's own pick wins (`runs::resolve_effort`).
    pub default_effort: String,
    /// Response style of new sessions: a built-in key or a style id, "" for none.
    /// A run's own pick or stored style wins (`response_styles::resolve_run_style`).
    pub default_style: String,
    /// Terminal app to open a project folder with, one of this OS's (`projects::terminal_apps`):
    /// "terminal" (Terminal.app) or "iterm" on macOS.
    pub terminal_app: String,
    /// Context-window meter: warn threshold as a percent string, e.g. "80".
    pub context_warn_percent: String,
    /// Context-window limit override in tokens ("" = auto-resolve from model).
    pub context_limit_override: String,
    /// Block new runs when the rolling 5h plan window reaches this percent
    /// ("" = don't enforce the 5h window).
    pub budget_5h_percent: String,
    /// Block new runs when the weekly plan window reaches this percent
    /// ("" = don't enforce the weekly window).
    pub budget_week_percent: String,
    /// Auto-balance the default Claude account across managed accounts, so plan
    /// usage spreads instead of hammering one login. "true" (default) | "false".
    /// When on, new runs that aren't pinned to a project/account follow whichever
    /// account the balancer keeps as default.
    pub claude_auto_balance: String,
    /// Minimum minutes between two automatic default-account swaps (cooldown).
    /// Default "10". Prevents thrashing when many runs finish close together.
    pub claude_auto_balance_min_swap_minutes: String,
    /// Only auto-swap the default when the lightest account's load is at least
    /// this many points below the current default's (hysteresis). Default "10".
    pub claude_auto_balance_swap_threshold_pct: String,
    /// Engine used by the "translate selection" feature ("claude" | "codex").
    pub translate_engine: String,
    /// Model used for translations ("" = a fast default, e.g. haiku for Claude).
    pub translate_model: String,
    /// Default target language for translations (code or name, e.g. "vi").
    pub translate_target_lang: String,
    /// Translation style: "natural" | "literal" | "technical" | "formal" | "casual".
    pub translate_style: String,
    /// Inject the built-in `puppeteer` MCP server (notes + session recall + project
    /// context + VPS) into every run. "true" (default) | "false".
    pub mcp_builtin_puppeteer_enabled: String,
    /// Inject the built-in `google` MCP server when an account is connected.
    /// "true" (default) | "false".
    pub mcp_builtin_google_enabled: String,
    /// What quitting does while sessions or leftover background processes are
    /// still running: "ask" (default) | "stop_all" | "keep_leftovers". Read by
    /// `procs::quit`.
    pub quit_background_policy: String,
    /// Newest app version whose "What's new" notes were offered ("" = none yet).
    /// Per machine: left out of a backup (see `src/lib/whatsNew.ts`).
    pub whats_new_seen_version: String,
    /// Command palette (⌘K) groups, pins and hidden items, stored under the
    /// `palette.config` row as JSON ("" = defaults). The UI owns the shape
    /// (`src/lib/commandPalette.ts`); the backend only round-trips the string.
    pub palette_config: String,
    /// Shortcuts of the native menu's actions the user changed, stored under the
    /// `keymap.config` row as JSON ("" = defaults). The UI owns the shape and the
    /// checks (`src/lib/keymap.ts`); the backend only round-trips the string.
    pub keymap_config: String,
}

/// `AppSettings` keys carried by a settings backup. `claude_path` / `codex_path` are left out:
/// they are machine-specific binary paths. Internal bookkeeping keys (usage snapshots, balancer
/// state, Inbox API token, the `feature._seeded` flag…) are never exported. The feature switches
/// travel too, but come from the registry: see [`is_portable_setting`].
pub const PORTABLE_SETTING_KEYS: &[&str] = &[
    "default_engine",
    "claude_model",
    "codex_model",
    "extra_args",
    "theme",
    "language",
    "color_theme",
    "animated_background",
    "saved_prompts",
    "default_permission_mode",
    "default_effort",
    "default_style",
    "terminal_app",
    "context_warn_percent",
    "context_limit_override",
    "budget_5h_percent",
    "budget_week_percent",
    "claude_auto_balance",
    "claude_auto_balance_min_swap_minutes",
    "claude_auto_balance_swap_threshold_pct",
    "translate_engine",
    "translate_model",
    "translate_target_lang",
    "translate_style",
    "mcp_builtin_puppeteer_enabled",
    "mcp_builtin_google_enabled",
    "quit_background_policy",
    // Focus (focus::settings); the calendar choice stays per machine.
    "focus.work_start",
    "focus.work_end",
    "focus.work_days",
    "focus.morning_time",
    "focus.notify_enabled",
    "focus.notify_kinds",
    "focus.calibration_enabled",
    "focus.ai_engine",
    "focus.ai_model",
];

/// Key prefixes whose every row travels in a settings backup, for UI preferences stored as a
/// family of keys (`palette.config`, `keymap.config`, …).
pub const PORTABLE_SETTING_PREFIXES: &[&str] = &["palette.", "keymap."];

/// Whether a settings backup carries `key`: [`PORTABLE_SETTING_KEYS`], the keys under
/// [`PORTABLE_SETTING_PREFIXES`] and every key of the feature registry (`feature.*` and the
/// legacy keys it reads, such as `inbox_api_enabled`).
pub fn is_portable_setting(key: &str) -> bool {
    PORTABLE_SETTING_KEYS.contains(&key)
        || PORTABLE_SETTING_PREFIXES.iter().any(|p| key.len() > p.len() && key.starts_with(p))
        || crate::features::owner(key).is_some()
}

#[tauri::command]
pub async fn get_settings(db: State<'_, Db>) -> Result<AppSettings, String> {
    let rows = sqlx::query("SELECT key, value FROM settings")
        .fetch_all(db.inner())
        .await
        .map_err(|e| e.to_string())?;

    let mut settings = AppSettings {
        default_engine: "claude".to_string(),
        claude_path: "claude".to_string(),
        codex_path: "codex".to_string(),
        claude_model: "".to_string(),
        codex_model: "".to_string(),
        extra_args: "".to_string(),
        theme: "system".to_string(),
        language: "en".to_string(),
        color_theme: "default".to_string(),
        animated_background: "true".to_string(),
        saved_prompts: "[]".to_string(),
        default_permission_mode: "default".to_string(),
        default_effort: String::new(),
        default_style: String::new(),
        terminal_app: String::new(),
        context_warn_percent: "80".to_string(),
        context_limit_override: "".to_string(),
        budget_5h_percent: "".to_string(),
        budget_week_percent: "".to_string(),
        claude_auto_balance: "true".to_string(),
        claude_auto_balance_min_swap_minutes: "10".to_string(),
        claude_auto_balance_swap_threshold_pct: "10".to_string(),
        translate_engine: "claude".to_string(),
        translate_model: "".to_string(),
        translate_target_lang: "vi".to_string(),
        translate_style: "natural".to_string(),
        mcp_builtin_puppeteer_enabled: "true".to_string(),
        mcp_builtin_google_enabled: "true".to_string(),
        quit_background_policy: "ask".to_string(),
        whats_new_seen_version: String::new(),
        palette_config: String::new(),
        keymap_config: String::new(),
    };

    for row in rows {
        let key: String = row.get("key");
        let value: String = row.get("value");
        match key.as_str() {
            "default_engine" => settings.default_engine = value,
            "claude_path" => settings.claude_path = value,
            "codex_path" => settings.codex_path = value,
            "claude_model" => settings.claude_model = value,
            "codex_model" => settings.codex_model = value,
            "extra_args" => settings.extra_args = value,
            "theme" => settings.theme = value,
            "language" => settings.language = value,
            "color_theme" => settings.color_theme = value,
            "animated_background" => settings.animated_background = value,
            "saved_prompts" => settings.saved_prompts = value,
            "default_permission_mode" => settings.default_permission_mode = value,
            "default_effort" => settings.default_effort = value,
            "default_style" => settings.default_style = value,
            "terminal_app" => settings.terminal_app = value,
            "context_warn_percent" => settings.context_warn_percent = value,
            "context_limit_override" => settings.context_limit_override = value,
            "budget_5h_percent" => settings.budget_5h_percent = value,
            "budget_week_percent" => settings.budget_week_percent = value,
            "claude_auto_balance" => settings.claude_auto_balance = value,
            "claude_auto_balance_min_swap_minutes" => {
                settings.claude_auto_balance_min_swap_minutes = value
            }
            "claude_auto_balance_swap_threshold_pct" => {
                settings.claude_auto_balance_swap_threshold_pct = value
            }
            "translate_engine" => settings.translate_engine = value,
            "translate_model" => settings.translate_model = value,
            "translate_target_lang" => settings.translate_target_lang = value,
            "translate_style" => settings.translate_style = value,
            "mcp_builtin_puppeteer_enabled" => settings.mcp_builtin_puppeteer_enabled = value,
            "mcp_builtin_google_enabled" => settings.mcp_builtin_google_enabled = value,
            "quit_background_policy" => settings.quit_background_policy = value,
            "whats_new_seen_version" => settings.whats_new_seen_version = value,
            "palette.config" => settings.palette_config = value,
            "keymap.config" => settings.keymap_config = value,
            _ => {}
        }
    }
    // Unset, or saved on another OS (a restored backup): this OS's default.
    settings.terminal_app =
        crate::commands::projects::resolve_terminal_app(std::env::consts::OS, &settings.terminal_app).to_string();

    Ok(settings)
}

/// Resolve the global default engine, falling back to "claude" when unset or
/// empty. Used by run-creation paths now that engine is no longer stored
/// per-project.
pub async fn resolve_default_engine(db: &Db) -> String {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = 'default_engine'")
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "claude".to_string())
}

#[tauri::command]
pub async fn update_setting(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    db: State<'_, Db>,
    key: String,
    value: String,
) -> Result<(), String> {
    // A remote desktop only changes the user-facing settings.
    if crate::remote::guard::from_remote_bridge(&webview) && !crate::remote::guard::remote_may_set_setting(&key) {
        return Err(crate::remote::guard::DENIED.to_string());
    }
    // A switch's own key (`animated_background`): tell the windows and services.
    if crate::features::owner(&key).is_some() {
        return crate::features::commands::write_legacy(&app, db.inner(), set_setting(db.inner(), &key, &value)).await;
    }
    set_setting(db.inner(), &key, &value).await
}

pub async fn set_setting(db: &Db, key: &str, value: &str) -> Result<(), String> {
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
