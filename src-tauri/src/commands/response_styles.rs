//! Response styles: a short text added to a session's system prompt so the
//! answers follow a tone or format ("Concise", "Explain for a junior", …).
//!
//! Every style is a row of `response_styles` (migrations 0011, 0018), referenced
//! by id: global when `project_id` is NULL, else usable only in that project.
//! The samples ([`BUILTIN_STYLES`]) are rows too, seeded by migration 0018 with
//! their key as id and `builtin_key`; the user edits or deletes them like any
//! other style and [`restore_builtins`] puts back the deleted ones (UX-36). Only
//! [`DEFAULT_STYLE_KEY`] ("No style") lives in code: it adds nothing, so there
//! is nothing to edit, and it cannot be deleted.
//!
//! A run keeps the style it resolved in `runs.style`, so a resume runs with it
//! again ([`resolve_run_style`]). Claude gets the style through the appended
//! system prompt, Codex through the thread's developer instructions (see
//! `runs::start_run_inner` / `resume_run_with`).

use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::db::Db;

/// The style meaning "no extra text": the engine's own way of answering.
pub const DEFAULT_STYLE_KEY: &str = "default";

/// Setting holding the style of new sessions (a style id, or `default`);
/// empty or unknown means none. Its UI comes with UX-18.
pub const DEFAULT_STYLE_SETTING: &str = "default_style";

/// A sample style as migration 0018 seeds it. Name and description are the
/// English UI strings: while a row still has them the UI shows the translated
/// ones. The body is English: it is read by the model.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinStyle {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub body: &'static str,
    /// Below the user's styles' (0, 1, …), so a restored sample goes back on top.
    pub position: i64,
}

/// The samples, in the UI's order. Must match migration 0018 (a test checks it).
pub const BUILTIN_STYLES: &[BuiltinStyle] = &[
    BuiltinStyle {
        key: "concise",
        name: "Concise",
        description: "Answer first, only the needed details, no filler.",
        body: "Be concise. Lead with the answer in one or two sentences, then only the details that are needed. \
No preamble, no recap of the question, no closing pleasantries. Prefer short bullet points over paragraphs. \
Code stays complete and unchanged.",
        position: -3,
    },
    BuiltinStyle {
        key: "explain_junior",
        name: "Explain for a junior",
        description: "Plain words, terms defined, concrete examples.",
        body: "Explain for a junior developer. Use short sentences and plain words; when a technical term is \
unavoidable, define it in one short sentence where it first appears. Prefer a concrete example over theory, \
and show before / after when something changes. Say why, not only what. Keep technical accuracy and code \
unchanged.",
        position: -2,
    },
    BuiltinStyle {
        key: "code_review",
        name: "Code review",
        description: "Verdict first, findings by severity with file and line.",
        body: "Answer as a careful code reviewer. Start with a one-line verdict. Then list findings ordered by \
severity (blocking, should fix, nit), each with the file and line, what breaks and in which scenario, and \
the suggested fix. Separate what you verified from what you assume. No praise or filler.",
        position: -1,
    },
];

/// Longest name / description / body accepted. The body goes into every turn's
/// system prompt, so it is kept to a few lines' worth.
const NAME_MAX_CHARS: usize = 80;
const DESCRIPTION_MAX_CHARS: usize = 300;
pub const BODY_MAX_CHARS: usize = 8_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct ResponseStyle {
    pub id: String,
    /// NULL: a global style.
    pub project_id: Option<String>,
    pub name: String,
    pub description: String,
    pub body: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
    /// The sample this row is (`concise`, …), `None` for the user's own styles.
    pub builtin_key: Option<String>,
    /// A sample whose name is still the seeded one: the UI shows it translated.
    #[sqlx(skip)]
    pub stock_name: bool,
    /// Same for the description.
    #[sqlx(skip)]
    pub stock_description: bool,
}

impl ResponseStyle {
    /// Fill the `stock_*` flags from the sample's seeded texts.
    fn with_stock_flags(mut self) -> Self {
        if let Some(seed) = self.builtin_key.as_deref().and_then(builtin) {
            self.stock_name = self.name == seed.name;
            self.stock_description = self.description == seed.description;
        }
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseStyleList {
    /// Global styles, then (when a project is given) that project's.
    pub styles: Vec<ResponseStyle>,
}

const COLUMNS: &str = "id, project_id, name, description, body, position, created_at, updated_at, builtin_key";

/// The sample with key `key`, as seeded.
pub fn builtin(key: &str) -> Option<&'static BuiltinStyle> {
    BUILTIN_STYLES.iter().find(|b| b.key == key)
}

/// Global styles, plus `project_id`'s own when given.
pub async fn list(db: &Db, project_id: Option<&str>) -> Result<Vec<ResponseStyle>, String> {
    let rows: Vec<ResponseStyle> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM response_styles WHERE project_id IS NULL OR project_id = ? \
         ORDER BY project_id IS NOT NULL, position, name COLLATE NOCASE"
    ))
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(ResponseStyle::with_stock_flags).collect())
}

async fn get(db: &Db, id: &str) -> Result<ResponseStyle, String> {
    sqlx::query_as::<_, ResponseStyle>(&format!("SELECT {COLUMNS} FROM response_styles WHERE id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .map(ResponseStyle::with_stock_flags)
        .ok_or_else(|| "Response style not found".to_string())
}

/// Trimmed, checked fields of a style.
fn validate(name: &str, description: &str, body: &str) -> Result<(String, String, String), String> {
    let name = name.trim();
    let description = description.trim();
    let body = body.trim();
    if name.is_empty() {
        return Err("The style needs a name".into());
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(format!("The name is longer than {NAME_MAX_CHARS} characters"));
    }
    if description.chars().count() > DESCRIPTION_MAX_CHARS {
        return Err(format!("The description is longer than {DESCRIPTION_MAX_CHARS} characters"));
    }
    if body.is_empty() {
        return Err("The style needs a text".into());
    }
    if body.chars().count() > BODY_MAX_CHARS {
        return Err(format!("The text is longer than {BODY_MAX_CHARS} characters"));
    }
    Ok((name.to_string(), description.to_string(), body.to_string()))
}

/// Whether another style of the same scope already has `name`.
async fn name_taken(db: &Db, project_id: Option<&str>, name: &str, except_id: Option<&str>) -> Result<bool, String> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM response_styles \
         WHERE IFNULL(project_id, '') = IFNULL(?, '') AND name = ? COLLATE NOCASE AND id IS NOT ?",
    )
    .bind(project_id)
    .bind(name)
    .bind(except_id)
    .fetch_one(db)
    .await
    .map(|n| n > 0)
    .map_err(|e| e.to_string())
}

pub async fn create(
    db: &Db,
    project_id: Option<&str>,
    name: &str,
    description: &str,
    body: &str,
) -> Result<ResponseStyle, String> {
    let (name, description, body) = validate(name, description, body)?;
    let project_id = project_id.map(str::trim).filter(|p| !p.is_empty());
    if let Some(pid) = project_id {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM projects WHERE id = ?")
            .bind(pid)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?;
        if exists.is_none() {
            return Err("Project not found".into());
        }
    }
    if name_taken(db, project_id, &name, None).await? {
        return Err(format!("A style named '{name}' already exists"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO response_styles (id, project_id, name, description, body, position, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, \
                 (SELECT IFNULL(MAX(position), -1) + 1 FROM response_styles WHERE IFNULL(project_id, '') = IFNULL(?, '')), \
                 ?, ?)",
    )
    .bind(&id)
    .bind(project_id)
    .bind(&name)
    .bind(&description)
    .bind(&body)
    .bind(project_id)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    get(db, &id).await
}

/// Edit a style's text. Its scope (global / project) does not change.
pub async fn update(db: &Db, id: &str, name: &str, description: &str, body: &str) -> Result<ResponseStyle, String> {
    let (name, description, body) = validate(name, description, body)?;
    let current = get(db, id).await?;
    if name_taken(db, current.project_id.as_deref(), &name, Some(id)).await? {
        return Err(format!("A style named '{name}' already exists"));
    }
    sqlx::query("UPDATE response_styles SET name = ?, description = ?, body = ?, updated_at = ? WHERE id = ?")
        .bind(&name)
        .bind(&description)
        .bind(&body)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    get(db, id).await
}

/// Delete a style (a sample too). A default that pointed at it goes back to
/// "No style": the `default_style` setting is emptied and a project's default
/// becomes `default`. Sessions that ran with it fall back to their defaults on
/// their next resume ([`resolve_run_style`] skips an unknown id).
pub async fn delete(db: &Db, id: &str) -> Result<(), String> {
    if id == DEFAULT_STYLE_KEY {
        return Err("\"No style\" cannot be deleted".into());
    }
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM response_styles WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE settings SET value = '' WHERE key = ? AND value = ?")
        .bind(DEFAULT_STYLE_SETTING)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE projects SET default_style = ? WHERE default_style = ?")
        .bind(DEFAULT_STYLE_KEY)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())
}

/// Put back the samples the user deleted, as seeded; one still there is left
/// as it is, edited or not. A sample whose name a global style of the user's
/// took meanwhile comes back as "Name (2)", "Name (3)", …. Returns the samples
/// put back.
pub async fn restore_builtins(db: &Db) -> Result<Vec<ResponseStyle>, String> {
    let mut restored = Vec::new();
    for seed in BUILTIN_STYLES {
        let present: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM response_styles WHERE id = ? OR builtin_key = ?")
            .bind(seed.key)
            .bind(seed.key)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?;
        if present > 0 {
            continue;
        }
        let mut name = seed.name.to_string();
        let mut n = 2;
        while name_taken(db, None, &name, None).await? {
            name = format!("{} ({n})", seed.name);
            n += 1;
        }
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO response_styles \
             (id, project_id, name, description, body, position, created_at, updated_at, builtin_key) \
             VALUES (?, NULL, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(seed.key)
        .bind(&name)
        .bind(seed.description)
        .bind(seed.body)
        .bind(seed.position)
        .bind(&now)
        .bind(&now)
        .bind(seed.key)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
        restored.push(get(db, seed.key).await?);
    }
    Ok(restored)
}

// ---- Resolution ---------------------------------------------------------------

/// The style a run uses: its reference (`key`, stored in `runs.style`) and the
/// text to inject (`None` for `default`, "No style").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStyle {
    pub key: String,
    pub body: Option<String>,
}

/// The text of style `key` as usable in `project_id`: "No style", a global
/// style (a sample, whose id is its key, included), or a style of that
/// project. `None` when there is no such style.
pub(crate) async fn lookup(db: &Db, project_id: &str, key: &str) -> Option<ResolvedStyle> {
    if key == DEFAULT_STYLE_KEY {
        return Some(ResolvedStyle { key: key.to_string(), body: None });
    }
    let row = sqlx::query("SELECT body FROM response_styles WHERE id = ? AND (project_id IS NULL OR project_id = ?)")
        .bind(key)
        .bind(project_id)
        .fetch_optional(db)
        .await
        .ok()??;
    let body: String = row.get("body");
    Some(ResolvedStyle { key: key.to_string(), body: Some(body).filter(|b| !b.trim().is_empty()) })
}

/// Whether style `key` can be used in `project_id`: "No style", a global
/// style, or one of that project's own.
pub async fn usable(db: &Db, project_id: &str, key: &str) -> bool {
    lookup(db, project_id, key).await.is_some()
}

/// The project's default style (`projects.default_style`, UX-19).
async fn project_default_style(db: &Db, project_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT default_style FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten()
}

async fn global_default_style(db: &Db) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(DEFAULT_STYLE_SETTING)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Resolve the style of a run in `project_id`, first usable of:
/// the per-run pick (`override_style`) > the style the run already has
/// (`run_style`, so a resume keeps it) > the project's default (UX-19) > the
/// global `default_style` setting. Empty or unknown references (a deleted
/// style, another project's style) count as unset. `None`: no style anywhere.
pub async fn resolve_run_style(
    db: &Db,
    project_id: &str,
    override_style: Option<&str>,
    run_style: Option<&str>,
) -> Option<ResolvedStyle> {
    let candidates = [
        override_style.map(str::to_string),
        run_style.map(str::to_string),
        project_default_style(db, project_id).await,
        global_default_style(db).await,
    ];
    for key in candidates.into_iter().flatten() {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        if let Some(found) = lookup(db, project_id, key).await {
            return Some(found);
        }
        tracing::warn!(event = "response_style_unknown", style = %key, "style not found; trying the next default");
    }
    None
}

/// The block a style's text is injected as, on both engines.
pub fn style_block(body: &str) -> String {
    format!(
        "<response-style>\nThe user has selected a response style. Apply it to all of your responses in this \
session. It changes only the tone and formatting of your prose, never the technical correctness of your \
answer or the contents of code blocks, and it does not override a direct instruction in the current turn.\n\n{}\n</response-style>",
        body.trim()
    )
}

/// The style block of a resolved style, if it adds any text.
pub fn resolved_block(style: Option<&ResolvedStyle>) -> Option<String> {
    style.and_then(|s| s.body.as_deref()).map(style_block)
}

/// Claude's appended system prompt: the non-empty parts in order (git-account
/// context, conductor framing, response style), or `None` when all are empty.
pub fn compose_append(parts: &[Option<&str>]) -> Option<String> {
    let parts: Vec<&str> = parts.iter().flatten().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

/// One-time block for a RESUMED session whose style changed since its last
/// turn, to prepend to the next message: Codex applies the developer
/// instructions of `thread/resume` only from the next compact, and Claude
/// resumes with the system prompt of its first turn (`runs::claude_prompt`).
/// `previous`: the run's stored `runs.style`.
/// `None` when the effective style did not change.
pub fn style_change_seed(previous: Option<&str>, next: Option<&ResolvedStyle>) -> Option<String> {
    let previous = previous.map(str::trim).filter(|k| !k.is_empty() && *k != DEFAULT_STYLE_KEY);
    let next_body = next.and_then(|s| s.body.as_deref().map(|b| (s.key.as_str(), b)));
    if previous == next_body.map(|(k, _)| k) {
        return None;
    }
    let text = match next_body {
        Some((_, body)) => format!("The response style of this session changed. From now on:\n\n{}", style_block(body)),
        None => "The response style chosen earlier in this session no longer applies: answer in your normal style.".to_string(),
    };
    Some(format!(
        "<session-framing>\nThe following are standing instructions for this whole session \
(they apply to this and every later turn; do not reply to them directly):\n\n{text}\n</session-framing>"
    ))
}

// ---- Commands -------------------------------------------------------------------

/// The styles usable in `project_id` (global only when `None`).
#[tauri::command]
pub async fn list_response_styles(db: State<'_, Db>, project_id: Option<String>) -> Result<ResponseStyleList, String> {
    Ok(ResponseStyleList { styles: list(db.inner(), project_id.as_deref()).await? })
}

#[tauri::command]
pub async fn create_response_style(
    db: State<'_, Db>,
    project_id: Option<String>,
    name: String,
    description: String,
    body: String,
) -> Result<ResponseStyle, String> {
    create(db.inner(), project_id.as_deref(), &name, &description, &body).await
}

#[tauri::command]
pub async fn update_response_style(
    db: State<'_, Db>,
    id: String,
    name: String,
    description: String,
    body: String,
) -> Result<ResponseStyle, String> {
    update(db.inner(), &id, &name, &description, &body).await
}

#[tauri::command]
pub async fn delete_response_style(db: State<'_, Db>, id: String) -> Result<(), String> {
    delete(db.inner(), &id).await
}

#[tauri::command]
pub async fn restore_builtin_styles(db: State<'_, Db>) -> Result<Vec<ResponseStyle>, String> {
    restore_builtins(db.inner()).await
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub async fn test_db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'a', '/a', 'c'), ('p2', 'b', '/b', 'c')")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    #[tokio::test]
    async fn crud_validates_and_keeps_names_unique_per_scope() {
        let db = test_db().await;
        let g = create(&db, None, "  Short ", "", " Be short. ").await.unwrap();
        assert_eq!((g.name.as_str(), g.body.as_str(), g.project_id.as_deref()), ("Short", "Be short.", None));
        // Same name in the same scope, any case: refused. In a project: fine.
        assert!(create(&db, None, "short", "", "x").await.is_err());
        let p = create(&db, Some("p1"), "Short", "", "Project text").await.unwrap();
        assert_eq!(p.project_id.as_deref(), Some("p1"));
        assert!(create(&db, Some("p1"), "SHORT", "", "x").await.is_err());
        assert!(create(&db, Some("nope"), "Other", "", "x").await.is_err());
        // Validation.
        assert!(create(&db, None, " ", "", "x").await.is_err());
        assert!(create(&db, None, "Empty", "", "  ").await.is_err());
        assert!(create(&db, None, "Long", "", &"x".repeat(BODY_MAX_CHARS + 1)).await.is_err());
        assert!(create(&db, None, &"n".repeat(NAME_MAX_CHARS + 1), "", "x").await.is_err());
        // Positions follow creation within a scope.
        let g2 = create(&db, None, "Second", "d", "y").await.unwrap();
        assert_eq!((g.position, g2.position), (0, 1));

        // The list of a project: global first (the samples on top), then its own;
        // never another project's.
        let names = |l: Vec<ResponseStyle>| l.into_iter().map(|s| (s.name, s.project_id)).collect::<Vec<_>>();
        assert_eq!(names(list(&db, Some("p1")).await.unwrap()), [
            ("Concise".to_string(), None),
            ("Explain for a junior".to_string(), None),
            ("Code review".to_string(), None),
            ("Short".to_string(), None),
            ("Second".to_string(), None),
            ("Short".to_string(), Some("p1".to_string())),
        ]);
        assert_eq!(list(&db, Some("p2")).await.unwrap().len(), 5);
        assert_eq!(list(&db, None).await.unwrap().len(), 5);

        // Update: renaming onto another style's name is refused; its own name is fine.
        assert!(update(&db, &g2.id, "short", "", "y").await.is_err());
        let edited = update(&db, &g2.id, "Second", "new", "z").await.unwrap();
        assert_eq!((edited.description.as_str(), edited.body.as_str()), ("new", "z"));
        assert!(update(&db, "missing", "a", "", "b").await.is_err());

        delete(&db, &g2.id).await.unwrap();
        assert_eq!(list(&db, None).await.unwrap().len(), 4);
        // Deleting the project drops its styles.
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert_eq!(list(&db, Some("p1")).await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn resolution_order_run_pick_then_run_then_global() {
        let db = test_db().await;
        let mine = create(&db, None, "Mine", "", "MY TEXT").await.unwrap();
        let other_project = create(&db, Some("p2"), "Theirs", "", "THEIR TEXT").await.unwrap();
        let key = |r: Option<ResolvedStyle>| r.map(|s| s.key);

        // Nothing anywhere.
        assert_eq!(resolve_run_style(&db, "p1", None, None).await, None);
        // The global setting applies when nothing else is set.
        sqlx::query("INSERT INTO settings (key, value) VALUES ('default_style', 'concise')").execute(&db).await.unwrap();
        assert_eq!(key(resolve_run_style(&db, "p1", None, None).await).as_deref(), Some("concise"));
        // The run's stored style beats the global default; the per-run pick beats both.
        assert_eq!(key(resolve_run_style(&db, "p1", None, Some(&mine.id)).await), Some(mine.id.clone()));
        assert_eq!(key(resolve_run_style(&db, "p1", Some("code_review"), Some(&mine.id)).await).as_deref(), Some("code_review"));
        // Blank, unknown, deleted, or another project's: skipped.
        assert_eq!(key(resolve_run_style(&db, "p1", Some("  "), Some("gone")).await).as_deref(), Some("concise"));
        assert_eq!(key(resolve_run_style(&db, "p1", Some(&other_project.id), None).await).as_deref(), Some("concise"));
        assert_eq!(key(resolve_run_style(&db, "p2", Some(&other_project.id), None).await), Some(other_project.id.clone()));

        // Built-in default resolves (so a run keeps "no style") but carries no text.
        let def = resolve_run_style(&db, "p1", Some("default"), None).await.unwrap();
        assert_eq!((def.key.as_str(), def.body.as_deref()), ("default", None));
        assert_eq!(resolved_block(Some(&def)), None);
        let own = resolve_run_style(&db, "p1", Some(&mine.id), None).await.unwrap();
        assert!(resolved_block(Some(&own)).unwrap().contains("MY TEXT"));
    }

    #[tokio::test]
    async fn project_default_sits_between_the_run_and_the_global_default() {
        let db = test_db().await;
        let team = create(&db, Some("p1"), "Team", "", "TEAM TEXT").await.unwrap();
        let key = |r: Option<ResolvedStyle>| r.map(|s| s.key);
        sqlx::query("INSERT INTO settings (key, value) VALUES ('default_style', 'concise')").execute(&db).await.unwrap();
        sqlx::query("UPDATE projects SET default_style = ? WHERE id = 'p1'").bind(&team.id).execute(&db).await.unwrap();

        // The project's default beats the global one, only in that project.
        assert_eq!(key(resolve_run_style(&db, "p1", None, None).await), Some(team.id.clone()));
        assert_eq!(key(resolve_run_style(&db, "p2", None, None).await).as_deref(), Some("concise"));
        // A session that already ran keeps its style; a pick beats everything.
        assert_eq!(key(resolve_run_style(&db, "p1", None, Some("code_review")).await).as_deref(), Some("code_review"));
        assert_eq!(key(resolve_run_style(&db, "p1", Some("default"), Some("code_review")).await).as_deref(), Some("default"));
        assert!(usable(&db, "p1", &team.id).await && !usable(&db, "p2", &team.id).await);
        // Its style deleted: the project's default becomes "No style" (UX-36).
        delete(&db, &team.id).await.unwrap();
        assert_eq!(key(resolve_run_style(&db, "p1", None, None).await).as_deref(), Some(DEFAULT_STYLE_KEY));
    }

    #[test]
    fn append_keeps_account_context_framing_and_style_in_order() {
        let style = style_block("Be short.");
        assert_eq!(
            compose_append(&[Some("ACCOUNT"), Some("FRAMING"), Some(&style)]).unwrap(),
            format!("ACCOUNT\n\nFRAMING\n\n{style}")
        );
        // Resume: no account context, framing + style.
        assert_eq!(compose_append(&[None, Some("FRAMING"), Some(&style)]).unwrap(), format!("FRAMING\n\n{style}"));
        assert_eq!(compose_append(&[None, None, Some(&style)]).unwrap(), style);
        assert_eq!(compose_append(&[Some("ACCOUNT"), Some(" "), None]).unwrap(), "ACCOUNT");
        assert_eq!(compose_append(&[None, None, None]), None);
        assert!(style.starts_with("<response-style>") && style.ends_with("Be short.\n</response-style>"));
    }

    #[tokio::test]
    async fn the_migration_seeds_the_samples_as_in_code() {
        let db = test_db().await;
        let samples: Vec<ResponseStyle> =
            list(&db, None).await.unwrap().into_iter().filter(|s| s.builtin_key.is_some()).collect();
        assert_eq!(samples.len(), BUILTIN_STYLES.len());
        for (row, seed) in samples.iter().zip(BUILTIN_STYLES) {
            assert_eq!(
                (row.id.as_str(), row.builtin_key.as_deref(), row.project_id.as_deref()),
                (seed.key, Some(seed.key), None)
            );
            assert_eq!((row.name.as_str(), row.description.as_str()), (seed.name, seed.description));
            assert_eq!((row.body.as_str(), row.position), (seed.body, seed.position), "{}", seed.key);
            assert!(row.stock_name && row.stock_description, "{}", seed.key);
        }
        assert!(builtin(DEFAULT_STYLE_KEY).is_none());
    }

    #[tokio::test]
    async fn the_migration_leaves_a_users_style_its_name() {
        // Migrate up to 0017 (through sqlx like the app does), add global styles of
        // the user's named like a sample, then run the rest.
        let dir = std::env::temp_dir().join(format!("puppeteer-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        for entry in std::fs::read_dir(&source).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.as_str() < "0018" {
                std::fs::copy(source.join(&name), dir.join(&name)).unwrap();
            }
        }
        let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate::Migrator::new(dir.as_path()).await.unwrap().run(&db).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        sqlx::query(
            "INSERT INTO response_styles (id, project_id, name, body, created_at, updated_at) \
             VALUES ('u1', NULL, 'concise', 'Mine', 'c', 'c'), ('u2', NULL, 'Concise (2)', 'Mine too', 'c', 'c')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let concise = get(&db, "concise").await.unwrap();
        assert_eq!((concise.name.as_str(), concise.stock_name), ("Concise (3)", false));
        assert_eq!(get(&db, "u1").await.unwrap().name, "concise");
        assert_eq!(list(&db, None).await.unwrap().len(), 5);
    }

    #[tokio::test]
    async fn old_references_to_a_sample_still_resolve() {
        let db = test_db().await;
        // A session that ran with 'concise' before the samples were rows.
        let found = resolve_run_style(&db, "p1", None, Some("concise")).await.unwrap();
        assert_eq!((found.key.as_str(), found.body.as_deref()), ("concise", Some(builtin("concise").unwrap().body)));
        // Edited: the session gets the new text; the UI shows the user's name.
        let edited = update(&db, "concise", "Short", "Concise, mine", "Be very short.").await.unwrap();
        assert_eq!((edited.builtin_key.as_deref(), edited.stock_name, edited.stock_description), (Some("concise"), false, false));
        let found = resolve_run_style(&db, "p1", None, Some("concise")).await.unwrap();
        assert_eq!(found.body.as_deref(), Some("Be very short."));
        // Only the body changed: name and description still show translated.
        let body_only = update(&db, "code_review", "Code review", builtin("code_review").unwrap().description, "Review.").await.unwrap();
        assert!(body_only.stock_name && body_only.stock_description);
    }

    #[tokio::test]
    async fn restore_puts_back_only_the_deleted_samples() {
        let db = test_db().await;
        // Nothing deleted: nothing to do.
        assert!(restore_builtins(&db).await.unwrap().is_empty());
        update(&db, "explain_junior", "Junior", "", "Explain simply.").await.unwrap();
        delete(&db, "concise").await.unwrap();
        delete(&db, "code_review").await.unwrap();
        assert!(lookup(&db, "p1", "concise").await.is_none());
        // The user took a sample's name meanwhile.
        create(&db, None, "Code review", "", "Mine").await.unwrap();

        let restored = restore_builtins(&db).await.unwrap();
        let got = restored.iter().map(|s| (s.id.as_str(), s.name.as_str())).collect::<Vec<_>>();
        assert_eq!(got, [("concise", "Concise"), ("code_review", "Code review (2)")]);
        assert_eq!(restored[0].body, builtin("concise").unwrap().body);
        assert!(restored[0].stock_name && !restored[1].stock_name);
        // The edited sample is left as it is; a second restore adds nothing.
        assert_eq!(get(&db, "explain_junior").await.unwrap().name, "Junior");
        assert!(restore_builtins(&db).await.unwrap().is_empty());
        assert_eq!(list(&db, None).await.unwrap().len(), 4);
        assert!(lookup(&db, "p1", "concise").await.is_some());
    }

    #[tokio::test]
    async fn deleting_a_default_style_sets_the_default_back_to_no_style() {
        let db = test_db().await;
        let team = create(&db, Some("p2"), "Team", "", "TEAM TEXT").await.unwrap();
        sqlx::query("INSERT INTO settings (key, value) VALUES ('default_style', 'concise')").execute(&db).await.unwrap();
        sqlx::query("UPDATE projects SET default_style = 'concise' WHERE id = 'p1'").execute(&db).await.unwrap();
        sqlx::query("UPDATE projects SET default_style = ? WHERE id = 'p2'").bind(&team.id).execute(&db).await.unwrap();
        let global = || async { global_default_style(&db).await };

        delete(&db, "concise").await.unwrap();
        assert_eq!(global().await.as_deref(), Some(""));
        assert_eq!(project_default_style(&db, "p1").await.as_deref(), Some(DEFAULT_STYLE_KEY));
        assert_eq!(project_default_style(&db, "p2").await, Some(team.id.clone()));
        assert_eq!(resolve_run_style(&db, "p1", None, None).await.map(|s| s.body), Some(None));

        delete(&db, &team.id).await.unwrap();
        assert_eq!(project_default_style(&db, "p2").await.as_deref(), Some(DEFAULT_STYLE_KEY));
        // "No style" is not a row and cannot be deleted.
        assert!(delete(&db, DEFAULT_STYLE_KEY).await.is_err());
        assert!(usable(&db, "p1", DEFAULT_STYLE_KEY).await);
    }

    #[test]
    fn seed_only_when_the_style_changed() {
        let s = |key: &str, body: Option<&str>| ResolvedStyle { key: key.into(), body: body.map(str::to_string) };
        let concise = s("concise", Some("Be short."));
        let none = s("default", None);
        // Unchanged (also "no style" spelled three ways).
        assert_eq!(style_change_seed(Some("concise"), Some(&concise)), None);
        assert_eq!(style_change_seed(None, None), None);
        assert_eq!(style_change_seed(Some("default"), Some(&none)), None);
        assert_eq!(style_change_seed(None, Some(&none)), None);
        // New or changed style: the block, wrapped like the framing seed.
        let seed = style_change_seed(Some("code_review"), Some(&concise)).unwrap();
        assert!(seed.starts_with("<session-framing>") && seed.contains("Be short.") && seed.ends_with("</session-framing>"));
        assert!(style_change_seed(None, Some(&concise)).is_some());
        // Style dropped.
        let dropped = style_change_seed(Some("concise"), Some(&none)).unwrap();
        assert!(dropped.contains("no longer applies"));
        assert_eq!(crate::conductor::strip_session_framing(&format!("{seed}\n\nhello")), "hello");
    }
}
