//! Response styles: a short text added to a session's system prompt so the
//! answers follow a tone or format ("Concise", "Explain for a junior", …).
//!
//! Two kinds, one reference space:
//! - built-in styles live in code ([`BUILTIN_STYLES`], read-only; their names
//!   are UI strings) and are referenced by key (`concise`);
//! - the user's styles are rows of `response_styles` (migration 0011),
//!   referenced by id: global when `project_id` is NULL, else usable only in
//!   that project.
//!
//! A run keeps the style it resolved in `runs.style`, so a resume runs with it
//! again ([`resolve_run_style`]). Claude gets the style through the appended
//! system prompt, Codex through the thread's developer instructions (see
//! `runs::start_run_inner` / `resume_run_with`).

use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::db::Db;

/// Built-in style meaning "no extra text": the engine's own way of answering.
pub const DEFAULT_STYLE_KEY: &str = "default";

/// Setting holding the style of new sessions (a built-in key or a style id);
/// empty or unknown means none. Its UI comes with UX-18.
pub const DEFAULT_STYLE_SETTING: &str = "default_style";

/// Built-in styles: key → prompt text (English: it is read by the model). The
/// order is the UI's. `default` adds nothing.
pub const BUILTIN_STYLES: &[(&str, &str)] = &[
    (DEFAULT_STYLE_KEY, ""),
    (
        "concise",
        "Be concise. Lead with the answer in one or two sentences, then only the details that are needed. \
No preamble, no recap of the question, no closing pleasantries. Prefer short bullet points over paragraphs. \
Code stays complete and unchanged.",
    ),
    (
        "explain_junior",
        "Explain for a junior developer. Use short sentences and plain words; when a technical term is \
unavoidable, define it in one short sentence where it first appears. Prefer a concrete example over theory, \
and show before / after when something changes. Say why, not only what. Keep technical accuracy and code \
unchanged.",
    ),
    (
        "code_review",
        "Answer as a careful code reviewer. Start with a one-line verdict. Then list findings ordered by \
severity (blocking, should fix, nit), each with the file and line, what breaks and in which scenario, and \
the suggested fix. Separate what you verified from what you assume. No praise or filler.",
    ),
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
}

#[derive(Debug, Clone, Serialize)]
pub struct BuiltinStyle {
    pub key: &'static str,
    pub body: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseStyleList {
    pub builtins: Vec<BuiltinStyle>,
    /// Global styles, then (when a project is given) that project's.
    pub styles: Vec<ResponseStyle>,
}

const COLUMNS: &str = "id, project_id, name, description, body, position, created_at, updated_at";

pub fn builtin_body(key: &str) -> Option<&'static str> {
    BUILTIN_STYLES.iter().find(|(k, _)| *k == key).map(|(_, b)| *b)
}

/// Global styles, plus `project_id`'s own when given.
pub async fn list(db: &Db, project_id: Option<&str>) -> Result<Vec<ResponseStyle>, String> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM response_styles WHERE project_id IS NULL OR project_id = ? \
         ORDER BY project_id IS NOT NULL, position, name COLLATE NOCASE"
    ))
    .bind(project_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())
}

async fn get(db: &Db, id: &str) -> Result<ResponseStyle, String> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM response_styles WHERE id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
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

/// Delete a style. Sessions that ran with it fall back to the default style on
/// their next resume ([`resolve_run_style`] skips an unknown id).
pub async fn delete(db: &Db, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM response_styles WHERE id = ?")
        .bind(id)
        .execute(db)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---- Resolution ---------------------------------------------------------------

/// The style a run uses: its reference (`key`, stored in `runs.style`) and the
/// text to inject (`None` for the built-in `default`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStyle {
    pub key: String,
    pub body: Option<String>,
}

/// The text of style `key` as usable in `project_id`: a built-in, a global
/// style, or a style of that project. `None` when there is no such style.
async fn lookup(db: &Db, project_id: &str, key: &str) -> Option<ResolvedStyle> {
    if let Some(body) = builtin_body(key) {
        return Some(ResolvedStyle { key: key.to_string(), body: Some(body.to_string()).filter(|b| !b.is_empty()) });
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

/// The project's default style. Seam for UX-19 (per-project session defaults):
/// no project-level setting exists yet, so this is always `None`.
async fn project_default_style(_db: &Db, _project_id: &str) -> Option<String> {
    None
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

/// One-time block for a RESUMED Codex thread whose style changed since its
/// developer instructions were set (Codex ignores them on `thread/resume`), to
/// prepend to the next turn. `previous`: the run's stored `runs.style`.
/// `None` when the effective style did not change.
pub fn codex_style_seed(previous: Option<&str>, next: Option<&ResolvedStyle>) -> Option<String> {
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

/// Built-ins and the styles usable in `project_id` (global only when `None`).
#[tauri::command]
pub async fn list_response_styles(db: State<'_, Db>, project_id: Option<String>) -> Result<ResponseStyleList, String> {
    Ok(ResponseStyleList {
        builtins: BUILTIN_STYLES.iter().map(|(key, body)| BuiltinStyle { key, body }).collect(),
        styles: list(db.inner(), project_id.as_deref()).await?,
    })
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

        // The list of a project: global first, then its own; never another project's.
        let names = |l: Vec<ResponseStyle>| l.into_iter().map(|s| (s.name, s.project_id)).collect::<Vec<_>>();
        assert_eq!(names(list(&db, Some("p1")).await.unwrap()), [
            ("Short".to_string(), None),
            ("Second".to_string(), None),
            ("Short".to_string(), Some("p1".to_string())),
        ]);
        assert_eq!(list(&db, Some("p2")).await.unwrap().len(), 2);
        assert_eq!(list(&db, None).await.unwrap().len(), 2);

        // Update: renaming onto another style's name is refused; its own name is fine.
        assert!(update(&db, &g2.id, "short", "", "y").await.is_err());
        let edited = update(&db, &g2.id, "Second", "new", "z").await.unwrap();
        assert_eq!((edited.description.as_str(), edited.body.as_str()), ("new", "z"));
        assert!(update(&db, "missing", "a", "", "b").await.is_err());

        delete(&db, &g2.id).await.unwrap();
        assert_eq!(list(&db, None).await.unwrap().len(), 1);
        // Deleting the project drops its styles.
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert_eq!(list(&db, Some("p1")).await.unwrap().len(), 1);
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

    #[test]
    fn builtins_have_text_except_default() {
        assert_eq!(BUILTIN_STYLES.len(), 4);
        assert_eq!(builtin_body("default"), Some(""));
        for key in ["concise", "explain_junior", "code_review"] {
            assert!(!builtin_body(key).unwrap().is_empty(), "{key}");
        }
        assert_eq!(builtin_body("nope"), None);
    }

    #[test]
    fn codex_seed_only_when_the_style_changed() {
        let s = |key: &str, body: Option<&str>| ResolvedStyle { key: key.into(), body: body.map(str::to_string) };
        let concise = s("concise", Some("Be short."));
        let none = s("default", None);
        // Unchanged (also "no style" spelled three ways).
        assert_eq!(codex_style_seed(Some("concise"), Some(&concise)), None);
        assert_eq!(codex_style_seed(None, None), None);
        assert_eq!(codex_style_seed(Some("default"), Some(&none)), None);
        assert_eq!(codex_style_seed(None, Some(&none)), None);
        // New or changed style: the block, wrapped like the framing seed.
        let seed = codex_style_seed(Some("code_review"), Some(&concise)).unwrap();
        assert!(seed.starts_with("<session-framing>") && seed.contains("Be short.") && seed.ends_with("</session-framing>"));
        assert!(codex_style_seed(None, Some(&concise)).is_some());
        // Style dropped.
        let dropped = codex_style_seed(Some("concise"), Some(&none)).unwrap();
        assert!(dropped.contains("no longer applies"));
        assert_eq!(crate::conductor::strip_session_framing(&format!("{seed}\n\nhello")), "hello");
    }
}
