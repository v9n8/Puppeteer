//! Structured quotes a prompt carried (UX-29): what each quote of a sent
//! message points at, so the message shows numbered cards that scroll back to
//! the quoted text.
//!
//! The model is not involved: the prompt text already holds the quotes as a
//! markdown blockquote, written by the composer exactly as before. These rows
//! are saved by the send itself ([`save`], from `send_user_message`), and the
//! prompt's log record names them with `quote_set`.
//!
//! A quote of an assistant message is anchored by the message's id (Claude's
//! `message.id`, which the transcript keeps too; the Codex sidecar stamps one on
//! each agent message), never by the quoted text, so a phrase that occurs twice
//! still leads to the occurrence that was selected. [`of_run`] also says where
//! each quoted message sits in the run log now (`source_offset`), or that it is
//! gone (a Rewind cut it, the log was purged or rebuilt without ids).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Db;

/// One quote as the composer sends it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct QuoteInput {
    pub kind: String,
    #[serde(default)]
    pub source_run_id: Option<String>,
    #[serde(default)]
    pub source_entry_id: Option<String>,
    #[serde(default)]
    pub block_index: Option<i64>,
    #[serde(default)]
    pub start_offset: Option<i64>,
    #[serde(default)]
    pub end_offset: Option<i64>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub line_start: Option<i64>,
    #[serde(default)]
    pub line_end: Option<i64>,
    pub excerpt: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct RunQuote {
    pub id: String,
    pub run_id: String,
    pub quote_set: String,
    pub position: i64,
    pub kind: String,
    pub source_run_id: Option<String>,
    pub source_entry_id: Option<String>,
    pub block_index: Option<i64>,
    pub start_offset: Option<i64>,
    pub end_offset: Option<i64>,
    pub path: Option<String>,
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
    pub excerpt: String,
    pub note: String,
    pub created_at: String,
    /// Byte offset of the quoted message's record in the run log now (a
    /// forward-read cursor); None when it is no longer there. Not stored.
    #[sqlx(skip)]
    pub source_offset: Option<u64>,
}

/// Most quotes kept per prompt, and the longest excerpt / note / id kept: the
/// prompt text carries the quotes in full, these rows only point at them.
const MAX_QUOTES: usize = 50;
const EXCERPT_MAX_CHARS: usize = 20_000;
const NOTE_MAX_CHARS: usize = 2_000;
const ID_MAX_CHARS: usize = 200;

const COLUMNS: &str = "id, run_id, quote_set, position, kind, source_run_id, source_entry_id, block_index, \
     start_offset, end_offset, path, line_start, line_end, excerpt, note, created_at";

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn short(s: Option<String>) -> Option<String> {
    s.map(|v| clip(v.trim(), ID_MAX_CHARS)).filter(|v| !v.is_empty())
}

fn non_negative(v: Option<i64>) -> Option<i64> {
    v.filter(|n| *n >= 0)
}

/// A quote as stored: an anchor that doesn't hold together is dropped down to
/// what is left of it (an entry quote without a message id, a file quote
/// without a path, become plain text), never refused: the prompt is already
/// on its way to the engine.
fn normalize(q: QuoteInput) -> Option<QuoteInput> {
    let excerpt = clip(&q.excerpt, EXCERPT_MAX_CHARS);
    if excerpt.trim().is_empty() {
        return None;
    }
    let note = clip(q.note.trim(), NOTE_MAX_CHARS);
    let text = QuoteInput { kind: "text".into(), excerpt: excerpt.clone(), note: note.clone(), ..Default::default() };
    match q.kind.as_str() {
        "entry" => {
            let (Some(entry), Some(start), Some(end)) =
                (short(q.source_entry_id), non_negative(q.start_offset), non_negative(q.end_offset))
            else {
                return Some(text);
            };
            if end < start {
                return Some(text);
            }
            Some(QuoteInput {
                kind: "entry".into(),
                source_run_id: short(q.source_run_id),
                source_entry_id: Some(entry),
                block_index: Some(non_negative(q.block_index).unwrap_or(0)),
                start_offset: Some(start),
                end_offset: Some(end),
                excerpt,
                note,
                ..Default::default()
            })
        }
        "file" => {
            let Some(path) = q.path.map(|p| clip(p.trim(), 4096)).filter(|p| !p.is_empty()) else {
                return Some(text);
            };
            let line_start = q.line_start.filter(|n| *n >= 1);
            let line_end = line_start.and(q.line_end.filter(|n| *n >= line_start.unwrap_or(1)).or(line_start));
            Some(QuoteInput {
                kind: "file".into(),
                path: Some(path),
                line_start,
                line_end,
                excerpt,
                note,
                ..Default::default()
            })
        }
        _ => Some(text),
    }
}

/// Save the quotes of the prompt whose log record carries `quote_set`.
/// Returns how many were kept. Unknown run: nothing saved.
pub async fn save(db: &Db, run_id: &str, quote_set: &str, quotes: Vec<QuoteInput>) -> Result<usize, String> {
    let kept: Vec<QuoteInput> = quotes.into_iter().filter_map(normalize).take(MAX_QUOTES).collect();
    if kept.is_empty() {
        return Ok(0);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let mut saved = 0;
    for (position, q) in kept.into_iter().enumerate() {
        let res = sqlx::query(&format!(
            "INSERT INTO run_quotes ({COLUMNS})
             SELECT ?, id, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ? FROM runs WHERE id = ?"
        ))
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(quote_set)
        .bind(position as i64)
        .bind(&q.kind)
        .bind(&q.source_run_id)
        .bind(&q.source_entry_id)
        .bind(q.block_index)
        .bind(q.start_offset)
        .bind(q.end_offset)
        .bind(&q.path)
        .bind(q.line_start)
        .bind(q.line_end)
        .bind(&q.excerpt)
        .bind(&q.note)
        .bind(&now)
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        saved += res.rows_affected() as usize;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(saved)
}

/// A run's quotes, prompt by prompt (oldest first), in their order inside the
/// prompt. `source_offset` is left unset: see [`locate_messages`].
pub async fn of_run(db: &Db, run_id: &str) -> Result<Vec<RunQuote>, String> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM run_quotes WHERE run_id = ? ORDER BY created_at, quote_set, position"
    ))
    .bind(run_id)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())
}

/// Where each of `ids` (assistant message ids) first appears in the log at
/// `path`: the byte offset its record starts at, counted like the log search,
/// so each one is a valid forward-read cursor. Ids not found are absent.
pub(crate) fn locate_messages(path: &Path, ids: &[&str]) -> std::io::Result<HashMap<String, u64>> {
    use std::io::{BufRead, BufReader};

    let mut found: HashMap<String, u64> = HashMap::new();
    if ids.is_empty() {
        return Ok(found);
    }
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut offset: u64 = 0;
    let mut raw: Vec<u8> = Vec::new();
    while found.len() < ids.len() {
        raw.clear();
        let n = reader.read_until(b'\n', &mut raw)?;
        if n == 0 {
            break;
        }
        let start = offset;
        offset += n as u64;
        let line = String::from_utf8_lossy(&raw[..n]);
        // Cheap reject before paying for a JSON parse.
        let wanted: Vec<&str> =
            ids.iter().copied().filter(|id| !found.contains_key(*id) && line.contains(*id)).collect();
        if wanted.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim_end()) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(mid) = v.pointer("/message/id").and_then(|m| m.as_str()) else {
            continue;
        };
        if wanted.contains(&mid) {
            found.insert(mid.to_string(), start);
        }
    }
    Ok(found)
}

/// The quotes of a run's prompts, each with where its quoted message sits in
/// the run log now. Read-only: a remote Controller may call it.
#[tauri::command]
pub async fn list_run_quotes(db: State<'_, Db>, run_id: String) -> Result<Vec<RunQuote>, String> {
    let mut quotes = of_run(db.inner(), &run_id).await?;
    let ids: Vec<&str> = {
        let mut ids: Vec<&str> = quotes
            .iter()
            .filter(|q| q.kind == "entry")
            .filter_map(|q| q.source_entry_id.as_deref())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    if ids.is_empty() {
        return Ok(quotes);
    }
    let row = crate::commands::runs::fetch_run_log_row(db.inner(), &run_id).await?;
    let found = match crate::commands::runs::resolve_log_source(&row, &run_id) {
        Some((_, path)) => locate_messages(&path, &ids).map_err(|e| format!("read log: {e}"))?,
        None => HashMap::new(),
    };
    for q in &mut quotes {
        q.source_offset = q.source_entry_id.as_ref().and_then(|id| found.get(id).copied());
    }
    Ok(quotes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::bookmarks::tests::db_with_run;

    fn entry(id: &str, start: i64, end: i64, excerpt: &str) -> QuoteInput {
        QuoteInput {
            kind: "entry".into(),
            source_run_id: Some("r1".into()),
            source_entry_id: Some(id.into()),
            block_index: Some(0),
            start_offset: Some(start),
            end_offset: Some(end),
            excerpt: excerpt.into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn quotes_are_saved_and_listed_per_prompt_in_order() {
        let db = db_with_run("r1").await;
        let file = QuoteInput {
            kind: "file".into(),
            path: Some("src/a.ts".into()),
            line_start: Some(3),
            line_end: Some(5),
            excerpt: "let a = 1".into(),
            note: "  why?  ".into(),
            ..Default::default()
        };
        assert_eq!(save(&db, "r1", "set-1", vec![entry("msg_1", 4, 9, "hello"), file]).await.unwrap(), 2);
        assert_eq!(save(&db, "r1", "set-2", vec![entry("msg_2", 0, 3, "abc")]).await.unwrap(), 1);

        let all = of_run(&db, "r1").await.unwrap();
        let shape: Vec<_> = all
            .iter()
            .map(|q| (q.quote_set.as_str(), q.position, q.kind.as_str(), q.excerpt.as_str(), q.note.as_str()))
            .collect();
        assert_eq!(shape, [
            ("set-1", 0, "entry", "hello", ""),
            ("set-1", 1, "file", "let a = 1", "why?"),
            ("set-2", 0, "entry", "abc", ""),
        ]);
        let first = &all[0];
        assert_eq!(
            (first.source_entry_id.as_deref(), first.block_index, first.start_offset, first.end_offset),
            (Some("msg_1"), Some(0), Some(4), Some(9)),
        );
        assert_eq!((all[1].path.as_deref(), all[1].line_start, all[1].line_end), (Some("src/a.ts"), Some(3), Some(5)));
    }

    #[tokio::test]
    async fn a_broken_anchor_is_kept_as_plain_text_and_a_blank_quote_dropped() {
        let db = db_with_run("r1").await;
        let mut no_id = entry("msg_1", 0, 3, "abc");
        no_id.source_entry_id = None;
        let backwards = entry("msg_1", 9, 3, "abc");
        let no_path = QuoteInput { kind: "file".into(), excerpt: "x".into(), ..Default::default() };
        let blank = entry("msg_1", 0, 3, "  ");
        let saved = save(&db, "r1", "s", vec![no_id, backwards, no_path, blank]).await.unwrap();
        assert_eq!(saved, 3);
        let kinds: Vec<_> = of_run(&db, "r1").await.unwrap().into_iter().map(|q| q.kind).collect();
        assert_eq!(kinds, ["text", "text", "text"]);
    }

    #[tokio::test]
    async fn quotes_of_an_unknown_run_are_not_saved() {
        let db = db_with_run("r1").await;
        assert_eq!(save(&db, "nope", "s", vec![entry("m", 0, 1, "a")]).await.unwrap(), 0);
        assert!(of_run(&db, "nope").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purging_the_run_drops_its_quotes() {
        let db = db_with_run("r1").await;
        save(&db, "r1", "s", vec![entry("m", 0, 1, "a")]).await.unwrap();
        sqlx::query("DELETE FROM runs WHERE id = 'r1'").execute(&db).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_quotes").fetch_one(&db).await.unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn messages_are_located_by_id_at_their_first_record() {
        let dir = std::env::temp_dir().join(format!("puppeteer-quotes-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("r.log");
        let lines = [
            r#"{"type":"user","message":{"role":"user","content":"mentions msg_2 in text"}}"#,
            r#"{"type":"assistant","message":{"id":"msg_1","content":[{"type":"text","text":"a"}]}}"#,
            "[stderr] noise",
            r#"{"type":"assistant","message":{"id":"msg_2","content":[{"type":"thinking","thinking":"t"}]}}"#,
            r#"{"type":"assistant","message":{"id":"msg_2","content":[{"type":"text","text":"b"}]}}"#,
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let found = locate_messages(&path, &["msg_1", "msg_2", "msg_gone"]).unwrap();
        let at = |i: usize| lines[..i].iter().map(|l| l.len() as u64 + 1).sum::<u64>();
        assert_eq!(found.get("msg_1"), Some(&at(1)));
        // The user line that only mentions the id is not the message.
        assert_eq!(found.get("msg_2"), Some(&at(3)));
        assert!(!found.contains_key("msg_gone"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
