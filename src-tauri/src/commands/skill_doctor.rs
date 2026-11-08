//! Skill Doctor: a static check of a library skill's SKILL.md and folder. No model is called.
//!
//! Skills break most often in their metadata, not their text: a vague `description` and the model
//! never loads the skill, a personal skill of the same name in HOME hides the library's, a link to
//! `scripts/run.py` that isn't there fails halfway through a run.
//!
//! Two layers, as in the AWOG doctor this is ported from:
//! - [`inspect`] is pure (no I/O): SKILL.md text, the folder's files and the other skills in, a
//!   list of issues out. Every rule lives there, and the tests sit on it.
//! - [`diagnose`] reads the disk (size and entry caps) and calls it.
//!
//! An issue is machine-readable (`rule` + `params`), never an English sentence: the UI builds the
//! text through i18n.

use crate::commands::project_links::{self, AtDest};
use crate::commands::skills::SKILL_ENGINES;
use crate::db::Db;
use crate::features;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

/// Prop gating `diagnose_skills`.
pub const FEATURE: &str = "library.skill_doctor";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    // Frontmatter / identity
    Unreadable,
    MissingName,
    MissingDescription,
    InvalidId,
    NameIdMismatch,
    UnknownFrontmatterKey,
    // Description: when the skill applies
    DescriptionTooShort,
    DescriptionTooLong,
    DescriptionNoTrigger,
    // Collisions between skills
    ShadowedId,
    DuplicateName,
    // Body
    BodyEmpty,
    BodyThin,
    BodyTooLarge,
    // Files in the skill folder
    BrokenLink,
    LinkOutsideFolder,
    OversizedAsset,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    pub rule: Rule,
    pub severity: Severity,
    /// For the UI's sentence: numbers or short, clipped strings, never the body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Stats {
    pub file_bytes: u64,
    pub body_chars: usize,
    pub description_chars: usize,
    pub asset_files: usize,
}

#[derive(Debug, Serialize)]
pub struct SkillDoctorReport {
    pub skill_id: String,
    /// The skill's folder name, which the engines use as its id.
    pub name: String,
    /// Unix milliseconds.
    pub checked_at: i64,
    pub issues: Vec<Issue>,
    pub stats: Stats,
}

// The description is all the model sees before deciding to load a skill: too short is too little
// signal, too long eats the skill catalogue of every turn.
pub const DESCRIPTION_MIN_CHARS: usize = 40;
pub const DESCRIPTION_MAX_CHARS: usize = 500;
pub const BODY_THIN_CHARS: usize = 200;
pub const BODY_LARGE_BYTES: u64 = 32 * 1024;
/// Above this SKILL.md isn't read at all (only `body-too-large`), so a huge file can't stall us.
pub const MAX_READ_BYTES: u64 = 512 * 1024;
pub const OVERSIZED_ASSET_BYTES: u64 = 1024 * 1024;
/// A skill folder is a few files, not a source tree.
const MAX_ASSET_ENTRIES: usize = 500;
const MAX_ASSET_DEPTH: usize = 2;

/// Keys Puppeteer reads (`target`) plus common Claude Code ones it ignores but that aren't mistakes.
const KNOWN_FRONTMATTER_KEYS: &[&str] = &[
    "name",
    "description",
    "target",
    "icon",
    "globs",
    "alwaysallow",
    "requiredsources",
    "allowed-tools",
    "license",
    "version",
    "model",
    "metadata",
];

/// Folders SKILL.md uses by convention: a backticked path under one of them almost surely points
/// into the skill folder, not into the user's repo.
const ASSET_FOLDER_HINTS: &[&str] = &["scripts", "references", "assets", "templates", "examples", "data", "bin"];

/// A file in the skill folder: its path relative to the folder (with `/`) and its size.
#[derive(Debug, Clone)]
pub struct Asset {
    pub path: String,
    pub bytes: u64,
}

/// A library skill, for collisions: its folder name and its frontmatter `name`.
#[derive(Debug, Clone)]
pub struct Sibling {
    pub id: String,
    pub name: String,
}

pub struct InspectInput<'a> {
    /// The folder name.
    pub id: &'a str,
    /// SKILL.md. `None`: missing, unreadable or too large to read.
    pub raw: Option<&'a str>,
    pub file_bytes: u64,
    pub assets: &'a [Asset],
    /// Every library skill, this one included.
    pub siblings: &'a [Sibling],
    /// Engines whose HOME holds another skill of this name that wins over the library's.
    pub shadowed_in: &'a [&'static str],
}

fn issue(rule: Rule, severity: Severity, params: Option<Value>) -> Issue {
    Issue { rule, severity, params }
}

/// Params go straight to the UI: keep strings short.
fn clip(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(max - 1).collect::<String>())
    }
}

fn kb(bytes: u64) -> u64 {
    (bytes as f64 / 1024.0).round() as u64
}

fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// "PDF Filler" → "pdf-filler", to compare a display name with the folder name.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            out.push(c);
            gap = false;
        } else {
            gap = true;
        }
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `word` in `hay` with a word boundary on both sides.
fn has_word(hay: &str, word: &str) -> bool {
    hay.match_indices(word).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + word.len()..].chars().next();
        !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
    })
}

/// Whether the description says WHEN to use the skill, not only what it does. English and
/// Vietnamese, since users write SKILL.md in both.
fn has_trigger(description: &str) -> bool {
    const WORDS: &[&str] = &[
        "when", "whenever", "if", "use this", "use when", "invoke", "trigger", "applies to",
        "for tasks", "for requests", "for questions",
    ];
    const PHRASES: &[&str] = &["khi ", "nếu ", "dùng cho", "áp dụng"];
    let lower = description.to_lowercase();
    WORDS.iter().any(|w| has_word(&lower, w)) || PHRASES.iter().any(|p| lower.contains(p))
}

// ---- Frontmatter -------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum FmValue {
    Str(String),
    List(Vec<String>),
}

/// The minimal SKILL.md frontmatter of the AWOG parser: flat `key: value`, where a value is a
/// string (plain, quoted, `|` / `>` block) or a list (`[a, b]` or `- a` lines). Keys in order.
fn parse_frontmatter(raw: &str) -> (Vec<(String, FmValue)>, String) {
    let lines: Vec<&str> = raw.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    if lines.first().map(|l| l.trim()) != Some("---") {
        return (Vec::new(), raw.to_string());
    }
    let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---").map(|i| i + 1) else {
        return (Vec::new(), raw.to_string());
    };
    let data = parse_flat_yaml(&lines[1..end]);
    let body = lines[end + 1..].join("\n").trim_start_matches('\n').to_string();
    (data, body)
}

fn parse_flat_yaml(lines: &[&str]) -> Vec<(String, FmValue)> {
    let mut out: Vec<(String, FmValue)> = Vec::new();
    let mut set = |key: &str, value: FmValue| match out.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value,
        None => out.push((key.to_string(), value)),
    };
    let indented = |l: &str| l.starts_with(char::is_whitespace);
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        // Indented lines belong to the key above (a nested map such as `metadata:`), not keys.
        if line.trim().is_empty() || line.trim().starts_with('#') || indented(line) {
            i += 1;
            continue;
        }
        let Some(colon) = line.find(':') else {
            i += 1;
            continue;
        };
        let key = line[..colon].trim();
        let rest = line[colon + 1..].trim();

        // Block scalar: `|` keeps newlines, `>` folds lines; an optional `-` / `+` is accepted.
        let indicator = rest.strip_suffix(['-', '+']).unwrap_or(rest);
        if indicator == "|" || indicator == ">" {
            let mut j = i + 1;
            while j < lines.len() && (lines[j].trim().is_empty() || indented(lines[j])) {
                j += 1;
            }
            let collected = &lines[i + 1..j];
            let indent = collected
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.len() - l.trim_start().len())
                .min();
            let value = match indent {
                None => String::new(),
                Some(indent) => {
                    let mut text: Vec<&str> = collected.iter().map(|l| l.get(indent..).unwrap_or("")).collect();
                    while text.last().is_some_and(|l| l.trim().is_empty()) {
                        text.pop();
                    }
                    if indicator == ">" {
                        fold_block_scalar(&text)
                    } else {
                        text.join("\n")
                    }
                }
            };
            set(key, FmValue::Str(value));
            i = j;
            continue;
        }

        if rest.is_empty() {
            // A block list on the next lines (`  - "*.ts"`).
            let mut items = Vec::new();
            let mut j = i + 1;
            while let Some(item) = lines.get(j).and_then(|l| block_item(l)) {
                items.push(unquote(item));
                j += 1;
            }
            if items.is_empty() {
                set(key, FmValue::Str(String::new()));
                i += 1;
            } else {
                set(key, FmValue::List(items));
                i = j;
            }
            continue;
        }

        if rest.starts_with('[') && rest.ends_with(']') && rest.len() >= 2 {
            let inner = rest[1..rest.len() - 1].trim();
            let items = if inner.is_empty() { Vec::new() } else { split_flow_list(inner).iter().map(|s| unquote(s)).collect() };
            set(key, FmValue::List(items));
            i += 1;
            continue;
        }

        set(key, FmValue::Str(unquote(rest)));
        i += 1;
    }
    out
}

/// The text of a `- item` line, or `None` when it isn't one.
fn block_item(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix('-')?;
    rest.starts_with(char::is_whitespace).then(|| rest.trim())
}

/// YAML `>`: consecutive lines join with a space, a blank line is a paragraph break.
fn fold_block_scalar(lines: &[&str]) -> String {
    let mut out = String::new();
    let mut prev_blank = true;
    for line in lines {
        if line.trim().is_empty() {
            out.push('\n');
            prev_blank = true;
        } else {
            if !prev_blank {
                out.push(' ');
            }
            out.push_str(line.trim());
            prev_blank = false;
        }
    }
    out.trim().to_string()
}

fn split_flow_list(inner: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in inner.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
                current.push(c);
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.push(c);
            }
            None if c == ',' => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            None => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

fn unquote(value: &str) -> String {
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')));
    if quoted {
        value[1..value.len() - 1].replace("\\\"", "\"").replace("\\'", "'")
    } else {
        value.to_string()
    }
}

// ---- Links to files in the skill folder ---------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct LocalRef {
    target: String,
    /// Weak evidence: a backticked path. Bodies often name the user's repo files (`src/index.ts`),
    /// so a weak ref is checked only when it looks like it points into the skill folder.
    weak: bool,
}

fn has_scheme(value: &str) -> bool {
    let mut chars = value.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    for c in chars {
        if c == ':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) {
            return false;
        }
    }
    false
}

fn looks_like_file(value: &str) -> bool {
    value.rsplit_once('.').is_some_and(|(_, ext)| (1..=8).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Candidate paths to files in the skill folder, from the body: markdown links `[x](scripts/a.py)`
/// (strong) and backticked paths `` `references/x.json` `` (weak). URLs, anchors and absolute
/// paths are skipped: they aren't files that ship with the skill.
fn collect_local_refs(body: &str) -> Vec<LocalRef> {
    let mut out: Vec<LocalRef> = Vec::new();
    let mut push = |target: &str, weak: bool| {
        let value = target.trim().split('#').next().unwrap_or("").split('?').next().unwrap_or("");
        if value.is_empty()
            || has_scheme(value)
            || value.starts_with('/')
            || value.starts_with('~')
            || !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-' | ' '))
            || !looks_like_file(value)
        {
            return;
        }
        match out.iter_mut().find(|r| r.target == value) {
            Some(previous) => previous.weak &= weak,
            None => out.push(LocalRef { target: value.to_string(), weak }),
        }
    };

    // `[text](target ...)`
    let mut i = 0;
    while let Some(open) = body[i..].find('[').map(|o| i + o) {
        i = open + 1;
        let Some(close) = body[open..].find(']').map(|c| open + c) else { break };
        let Some(after) = body[close + 1..].strip_prefix('(') else { continue };
        let target_len = after.find(|c: char| c == ')' || c.is_whitespace()).unwrap_or(after.len());
        if target_len == 0 {
            continue;
        }
        let Some(end) = after.find(')') else { continue };
        push(&after[..target_len], false);
        i = close + 2 + end + 1;
    }

    // `` `path` `` on one line, at most 120 characters.
    let mut i = 0;
    while let Some(open) = body[i..].find('`').map(|o| i + o) {
        i = open + 1;
        let Some(len) = body[open + 1..].find(['`', '\n']) else { break };
        let inner = &body[open + 1..open + 1 + len];
        if body[open + 1 + len..].starts_with('`') && !inner.is_empty() && inner.chars().count() <= 120 {
            let token = inner.trim();
            if token.contains('/') || token.starts_with("./") {
                push(token, true);
            }
            i = open + 1 + len + 1;
        }
    }
    out
}

/// `path::normalize` for a relative posix path (`a/../b` → `b`, `../x` stays).
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if out.last().is_some_and(|p| *p != "..") => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

fn escapes_folder(rel: &str) -> bool {
    let normalized = normalize(&rel.replace('\\', "/"));
    normalized == ".." || normalized.starts_with("../")
}

/// A weak ref is checked when it says it is relative (`./`), sits under a folder the skill has,
/// or uses one of the folder names SKILL.md uses by convention.
fn should_check_weak_ref(target: &str, asset_dirs: &[&str]) -> bool {
    let first = target.split('/').next().unwrap_or("");
    target.starts_with("./") || asset_dirs.contains(&first) || ASSET_FOLDER_HINTS.contains(&first)
}

// ---- Rules -------------------------------------------------------------------------------------

fn check_frontmatter(input: &InspectInput, data: &[(String, FmValue)], issues: &mut Vec<Issue>) -> (String, String) {
    let text = |key: &str| match data.iter().find(|(k, _)| k == key) {
        Some((_, FmValue::Str(s))) => s.trim().to_string(),
        _ => String::new(),
    };
    let (name, description) = (text("name"), text("description"));

    if name.is_empty() {
        issues.push(issue(Rule::MissingName, Severity::Error, None));
    }
    if description.is_empty() {
        issues.push(issue(Rule::MissingDescription, Severity::Error, None));
    }
    let valid_id = is_valid_id(input.id);
    if !valid_id {
        issues.push(issue(Rule::InvalidId, Severity::Error, Some(json!({ "id": clip(input.id, 80) }))));
    }
    if !name.is_empty() && valid_id && slugify(&name) != input.id {
        issues.push(issue(Rule::NameIdMismatch, Severity::Info, Some(json!({ "name": clip(&name, 80), "id": input.id }))));
    }
    for (key, _) in data {
        if !KNOWN_FRONTMATTER_KEYS.contains(&key.to_lowercase().as_str()) {
            issues.push(issue(Rule::UnknownFrontmatterKey, Severity::Info, Some(json!({ "key": clip(key, 40) }))));
        }
    }
    (name, description)
}

fn check_description(description: &str, issues: &mut Vec<Issue>) {
    if description.is_empty() {
        return;
    }
    let chars = description.chars().count();
    if chars < DESCRIPTION_MIN_CHARS {
        issues.push(issue(Rule::DescriptionTooShort, Severity::Warn, Some(json!({ "chars": chars, "min": DESCRIPTION_MIN_CHARS }))));
    }
    if chars > DESCRIPTION_MAX_CHARS {
        issues.push(issue(Rule::DescriptionTooLong, Severity::Warn, Some(json!({ "chars": chars, "max": DESCRIPTION_MAX_CHARS }))));
    }
    if !has_trigger(description) {
        issues.push(issue(Rule::DescriptionNoTrigger, Severity::Warn, None));
    }
}

fn check_collisions(input: &InspectInput, name: &str, issues: &mut Vec<Issue>) {
    for engine in input.shadowed_in {
        issues.push(issue(
            Rule::ShadowedId,
            Severity::Warn,
            Some(json!({ "id": input.id, "other_source": "global", "engine": engine })),
        ));
    }
    if name.is_empty() {
        return;
    }
    let lower = name.to_lowercase();
    for sibling in input.siblings.iter().filter(|s| s.id != input.id) {
        if sibling.name.trim().to_lowercase() == lower {
            issues.push(issue(
                Rule::DuplicateName,
                Severity::Warn,
                Some(json!({ "name": clip(name, 80), "other_id": sibling.id })),
            ));
        }
    }
}

fn check_assets(input: &InspectInput, body: &str, issues: &mut Vec<Issue>) {
    let asset_dirs: Vec<&str> =
        input.assets.iter().filter_map(|a| a.path.split('/').next()).filter(|d| !d.is_empty()).collect();
    for r in collect_local_refs(body) {
        if escapes_folder(&r.target) {
            issues.push(issue(Rule::LinkOutsideFolder, Severity::Warn, Some(json!({ "target": clip(&r.target, 80) }))));
            continue;
        }
        if r.weak && !should_check_weak_ref(&r.target, &asset_dirs) {
            continue;
        }
        let normalized = normalize(r.target.strip_prefix("./").unwrap_or(&r.target));
        if !input.assets.iter().any(|a| a.path == normalized) {
            issues.push(issue(Rule::BrokenLink, Severity::Warn, Some(json!({ "target": clip(&r.target, 80) }))));
        }
    }
    for asset in input.assets {
        if asset.bytes > OVERSIZED_ASSET_BYTES {
            issues.push(issue(
                Rule::OversizedAsset,
                Severity::Info,
                Some(json!({ "path": clip(&asset.path, 80), "kb": kb(asset.bytes) })),
            ));
        }
    }
}

/// Every doctor rule, without I/O.
pub fn inspect(input: &InspectInput) -> (Vec<Issue>, Stats) {
    let mut issues = Vec::new();
    let too_large = |bytes: u64| {
        issue(Rule::BodyTooLarge, Severity::Warn, Some(json!({ "kb": kb(bytes), "max_kb": kb(BODY_LARGE_BYTES) })))
    };

    let Some(raw) = input.raw else {
        issues.push(issue(Rule::Unreadable, Severity::Error, Some(json!({ "bytes": input.file_bytes }))));
        if input.file_bytes > MAX_READ_BYTES {
            issues.push(too_large(input.file_bytes));
        }
        return (issues, Stats { file_bytes: input.file_bytes, ..Stats::default() });
    };

    let (data, body) = parse_frontmatter(raw);
    let (name, description) = check_frontmatter(input, &data, &mut issues);
    check_description(&description, &mut issues);
    check_collisions(input, &name, &mut issues);

    let body = body.trim();
    let body_chars = body.chars().count();
    if body_chars == 0 {
        issues.push(issue(Rule::BodyEmpty, Severity::Error, None));
    } else if body_chars < BODY_THIN_CHARS {
        issues.push(issue(Rule::BodyThin, Severity::Warn, Some(json!({ "chars": body_chars, "min": BODY_THIN_CHARS }))));
    }
    if input.file_bytes > BODY_LARGE_BYTES {
        issues.push(too_large(input.file_bytes));
    }

    check_assets(input, body, &mut issues);

    let stats = Stats {
        file_bytes: input.file_bytes,
        body_chars,
        description_chars: description.chars().count(),
        asset_files: input.assets.len(),
    };
    (issues, stats)
}

// ---- I/O ---------------------------------------------------------------------------------------

/// SKILL.md's size and text; the text is `None` when it is missing, not UTF-8 or too large.
fn read_skill_md(folder: &Path) -> (u64, Option<String>) {
    let file = folder.join("SKILL.md");
    let Ok(meta) = fs::metadata(&file) else { return (0, None) };
    let bytes = meta.len();
    let raw = if bytes <= MAX_READ_BYTES { fs::read_to_string(&file).ok() } else { None };
    (bytes, raw)
}

/// Files in the skill folder but its SKILL.md, capped in count and depth. Symlinks are skipped and
/// an unreadable subfolder is left out.
fn list_assets(folder: &Path) -> Vec<Asset> {
    let mut out = Vec::new();
    let mut queue = VecDeque::from([(folder.to_path_buf(), 0usize)]);
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if out.len() >= MAX_ASSET_ENTRIES {
                return out;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                if depth + 1 < MAX_ASSET_DEPTH {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            if !kind.is_file() || (depth == 0 && entry.file_name() == "SKILL.md") {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let Ok(rel) = path.strip_prefix(folder) else { continue };
            let rel: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            out.push(Asset { path: rel.join("/"), bytes: meta.len() });
        }
    }
    out
}

/// Engines whose HOME folder holds a skill of this name that isn't Puppeteer's own link and wins
/// over the project's (the Allocation matrix's "shadowed" chip).
fn shadowed_in(home: Option<&Path>, name: &str, source: &Path) -> Vec<&'static str> {
    let Some(home) = home else { return Vec::new() };
    SKILL_ENGINES
        .iter()
        .filter(|e| e.home_wins)
        .filter_map(|e| {
            let dest = home.join(e.home_dir?).join(name);
            (dest.exists() && project_links::at_dest(home, &dest, source) != AtDest::OwnLink).then_some(e.id)
        })
        .collect()
}

/// Check one library skill (`skill_id`) or all of them.
pub async fn diagnose(db: &Db, home: Option<PathBuf>, skill_id: Option<&str>) -> Result<Vec<SkillDoctorReport>, String> {
    use sqlx::Row;
    features::require(db, FEATURE).await?;
    let skills: Vec<(String, String, PathBuf)> = sqlx::query("SELECT id, name, source_path FROM skills ORDER BY name")
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .map(|r| (r.get("id"), r.get("name"), PathBuf::from(r.get::<String, _>("source_path"))))
        .collect();
    if let Some(id) = skill_id {
        if !skills.iter().any(|s| s.0 == id) {
            return Err(format!("Skill not found: {}", id));
        }
    }
    let skill_id = skill_id.map(str::to_string);

    tokio::task::spawn_blocking(move || {
        let read: Vec<(u64, Option<String>)> = skills.iter().map(|(_, _, source)| read_skill_md(source)).collect();
        let siblings: Vec<Sibling> = skills
            .iter()
            .zip(&read)
            .map(|((_, name, _), (_, raw))| {
                let (data, _) = raw.as_deref().map(parse_frontmatter).unwrap_or_default();
                let name_fm = match data.iter().find(|(k, _)| k == "name") {
                    Some((_, FmValue::Str(s))) => s.clone(),
                    _ => String::new(),
                };
                Sibling { id: name.clone(), name: name_fm }
            })
            .collect();
        skills
            .iter()
            .zip(&read)
            .filter(|((id, _, _), _)| skill_id.as_ref().is_none_or(|want| want == id))
            .map(|((id, name, source), (file_bytes, raw))| {
                let assets = if raw.is_some() { list_assets(source) } else { Vec::new() };
                let shadowed = shadowed_in(home.as_deref(), name, source);
                let (issues, stats) = inspect(&InspectInput {
                    id: name,
                    raw: raw.as_deref(),
                    file_bytes: *file_bytes,
                    assets: &assets,
                    siblings: &siblings,
                    shadowed_in: &shadowed,
                });
                SkillDoctorReport {
                    skill_id: id.clone(),
                    name: name.clone(),
                    checked_at: chrono::Utc::now().timestamp_millis(),
                    issues,
                    stats,
                }
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Read-only, remote-callable: the doctor reports for one skill (`skill_id`) or every skill.
#[tauri::command]
pub async fn diagnose_skills(
    app: AppHandle,
    db: State<'_, Db>,
    skill_id: Option<String>,
) -> Result<Vec<SkillDoctorReport>, String> {
    diagnose(db.inner(), app.path().home_dir().ok(), skill_id.as_deref()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // A healthy SKILL.md as the baseline: each test changes only what it checks.
    const GOOD_DESCRIPTION: &str = "Use this when the user asks to fill in a PDF form or extract fields from a PDF file.";
    const GOOD_BODY: &str = "# PDF filler

Steps to follow when the user hands you a PDF:

1. Read the form fields with the helper script.
2. Map each answer onto its field name.
3. Write the filled document next to the original file.

Keep the original file untouched; always write a copy.";

    fn good_frontmatter() -> String {
        format!("---\nname: Pdf Filler\ndescription: \"{GOOD_DESCRIPTION}\"\n---")
    }

    fn with_frontmatter(fm: &str) -> String {
        format!("{fm}\n{GOOD_BODY}\n")
    }

    fn with_body(body: &str) -> String {
        format!("{}\n{body}", good_frontmatter())
    }

    fn self_sibling() -> Vec<Sibling> {
        vec![Sibling { id: "pdf-filler".into(), name: "Pdf Filler".into() }]
    }

    /// `pdf-filler` with `raw`, no files, only itself in the library, nothing in HOME.
    fn run(raw: &str) -> (Vec<Issue>, Stats) {
        run_with(Some(raw), raw.len() as u64, &[], &self_sibling(), &[], "pdf-filler")
    }

    fn run_with(
        raw: Option<&str>,
        file_bytes: u64,
        assets: &[Asset],
        siblings: &[Sibling],
        shadowed_in: &[&'static str],
        id: &str,
    ) -> (Vec<Issue>, Stats) {
        inspect(&InspectInput { id, raw, file_bytes, assets, siblings, shadowed_in })
    }

    fn rules(issues: &[Issue]) -> Vec<Rule> {
        issues.iter().map(|i| i.rule).collect()
    }

    fn find(issues: &[Issue], rule: Rule) -> &Issue {
        issues.iter().find(|i| i.rule == rule).unwrap_or_else(|| panic!("no {rule:?} in {issues:?}"))
    }

    fn asset(path: &str, bytes: u64) -> Asset {
        Asset { path: path.into(), bytes }
    }

    // ---- Healthy skill

    #[test]
    fn a_healthy_skill_has_no_issue_and_its_stats() {
        let (issues, stats) = run(&with_frontmatter(&good_frontmatter()));
        assert_eq!(issues, vec![]);
        assert_eq!(stats.description_chars, GOOD_DESCRIPTION.chars().count());
        assert_eq!(stats.body_chars, GOOD_BODY.chars().count());
        assert_eq!(stats.asset_files, 0);
    }

    #[test]
    fn the_seventeen_rules_serialize_as_kebab_case() {
        use Rule::*;
        let all = [
            Unreadable, MissingName, MissingDescription, InvalidId, NameIdMismatch, UnknownFrontmatterKey,
            DescriptionTooShort, DescriptionTooLong, DescriptionNoTrigger, ShadowedId, DuplicateName,
            BodyEmpty, BodyThin, BodyTooLarge, BrokenLink, LinkOutsideFolder, OversizedAsset,
        ];
        let names: Vec<String> = all.iter().map(|r| serde_json::to_value(r).unwrap().as_str().unwrap().to_string()).collect();
        assert_eq!(names.len(), 17);
        assert_eq!(names[0], "unreadable");
        assert_eq!(names[5], "unknown-frontmatter-key");
        assert_eq!(names[16], "oversized-asset");
        let json = serde_json::to_value(issue(Rule::BodyEmpty, Severity::Error, None)).unwrap();
        assert_eq!(json, serde_json::json!({ "rule": "body-empty", "severity": "error" }));
    }

    // ---- Frontmatter

    #[test]
    fn missing_name_and_description_are_errors() {
        let (issues, _) = run(&with_frontmatter("---\nicon: \"📄\"\n---"));
        assert!(rules(&issues).contains(&Rule::MissingName));
        assert!(rules(&issues).contains(&Rule::MissingDescription));
        assert_eq!(find(&issues, Rule::MissingName).severity, Severity::Error);
    }

    #[test]
    fn no_frontmatter_at_all_is_caught_too() {
        let found = rules(&run(GOOD_BODY).0);
        assert!(found.contains(&Rule::MissingName));
        assert!(found.contains(&Rule::MissingDescription));
    }

    #[test]
    fn a_folder_name_of_the_wrong_shape_is_an_error() {
        let raw = with_frontmatter(&good_frontmatter());
        for id in ["PDF Filler", "pdf_filler", "-pdf", ""] {
            let (issues, _) = run_with(Some(&raw), raw.len() as u64, &[], &[], &[], id);
            assert_eq!(find(&issues, Rule::InvalidId).severity, Severity::Error, "{id}");
        }
        let (issues, _) = run_with(Some(&raw), raw.len() as u64, &[], &[], &[], "pdf-filler-2");
        assert!(!rules(&issues).contains(&Rule::InvalidId));
    }

    #[test]
    fn an_unknown_key_is_reported_but_known_cli_and_puppeteer_keys_are_not() {
        let found = rules(&run(&with_frontmatter("---\nname: Pdf Filler\ndescriptio: nope\n---")).0);
        assert!(found.contains(&Rule::UnknownFrontmatterKey));

        let fm = format!(
            "---\nname: Pdf Filler\ndescription: \"{GOOD_DESCRIPTION}\"\nallowed-tools: Bash\nlicense: MIT\ntarget: both\nmetadata:\n  author: me\n  tags: x\n---"
        );
        assert_eq!(run(&with_frontmatter(&fm)).0, vec![]);
    }

    #[test]
    fn a_display_name_unlike_the_folder_is_only_info() {
        let fm = format!("---\nname: Invoice Parser\ndescription: \"{GOOD_DESCRIPTION}\"\n---");
        let (issues, _) = run(&with_frontmatter(&fm));
        let mismatch = find(&issues, Rule::NameIdMismatch);
        assert_eq!(mismatch.severity, Severity::Info);
        assert_eq!(mismatch.params.as_ref().unwrap()["id"], "pdf-filler");
    }

    // ---- Description

    #[test]
    fn a_short_description_is_reported_with_its_length() {
        let (issues, _) = run(&with_frontmatter("---\nname: Pdf Filler\ndescription: Fills PDFs when asked.\n---"));
        let params = find(&issues, Rule::DescriptionTooShort).params.clone().unwrap();
        assert_eq!(params["min"], DESCRIPTION_MIN_CHARS);
        assert!(params["chars"].as_u64().unwrap() < DESCRIPTION_MIN_CHARS as u64);
    }

    #[test]
    fn a_description_without_a_trigger_is_reported() {
        let fm = "---\nname: Pdf Filler\ndescription: \"A helper that fills PDF forms and extracts their fields for you.\"\n---";
        assert!(rules(&run(&with_frontmatter(fm)).0).contains(&Rule::DescriptionNoTrigger));
    }

    #[test]
    fn a_vietnamese_description_with_khi_has_a_trigger() {
        let fm = "---\nname: Pdf Filler\ndescription: \"Dùng khi người dùng cần điền biểu mẫu PDF hoặc trích xuất trường dữ liệu.\"\n---";
        assert!(!rules(&run(&with_frontmatter(fm)).0).contains(&Rule::DescriptionNoTrigger));
    }

    #[test]
    fn trigger_words_need_word_boundaries() {
        assert!(has_trigger("Whenever a PDF shows up"));
        assert!(has_trigger("Fill forms. Use When asked"));
        assert!(has_trigger("Applies to every invoice"));
        assert!(has_trigger("Nếu có PDF"));
        // "if" inside "verify", "trigger" inside "triggers", "when" inside "somewhen".
        assert!(!has_trigger("Verify triggers somewhen"));
    }

    #[test]
    fn a_long_description_is_reported() {
        let long = format!("Use this when {}", "x".repeat(600));
        let fm = format!("---\nname: Pdf Filler\ndescription: \"{long}\"\n---");
        let (issues, _) = run(&with_frontmatter(&fm));
        assert_eq!(find(&issues, Rule::DescriptionTooLong).params.as_ref().unwrap()["max"], DESCRIPTION_MAX_CHARS);
    }

    #[test]
    fn a_folded_block_description_is_read_whole() {
        let fm = "---\nname: Pdf Filler\ndescription: >-\n  Use this when the user asks to fill in a PDF form\n  or extract fields from a PDF file.\n---";
        let (issues, stats) = run(&with_frontmatter(fm));
        assert_eq!(issues, vec![]);
        assert_eq!(stats.description_chars, GOOD_DESCRIPTION.chars().count());
    }

    // ---- Collisions

    #[test]
    fn a_winning_home_skill_of_the_same_name_is_shadowed_id() {
        let raw = with_frontmatter(&good_frontmatter());
        let (issues, _) = run_with(Some(&raw), raw.len() as u64, &[], &self_sibling(), &["claude"], "pdf-filler");
        let params = find(&issues, Rule::ShadowedId).params.clone().unwrap();
        assert_eq!(params, serde_json::json!({ "id": "pdf-filler", "other_source": "global", "engine": "claude" }));
    }

    #[test]
    fn the_same_name_on_another_skill_is_duplicate_name() {
        let raw = with_frontmatter(&good_frontmatter());
        let siblings = vec![
            Sibling { id: "pdf-filler".into(), name: "Pdf Filler".into() },
            Sibling { id: "pdf-tools".into(), name: " pdf filler ".into() },
        ];
        let (issues, _) = run_with(Some(&raw), raw.len() as u64, &[], &siblings, &[], "pdf-filler");
        assert_eq!(find(&issues, Rule::DuplicateName).params.as_ref().unwrap()["other_id"], "pdf-tools");
    }

    #[test]
    fn the_skill_itself_among_its_siblings_is_no_collision() {
        let found = rules(&run(&with_frontmatter(&good_frontmatter())).0);
        assert!(!found.contains(&Rule::ShadowedId));
        assert!(!found.contains(&Rule::DuplicateName));
    }

    // ---- Body

    #[test]
    fn an_empty_body_is_an_error_not_thin() {
        let found = rules(&run(&format!("{}\n", good_frontmatter())).0);
        assert!(found.contains(&Rule::BodyEmpty));
        assert!(!found.contains(&Rule::BodyThin));
    }

    #[test]
    fn a_thin_body_is_a_warning() {
        let (issues, _) = run(&with_body("Fill the form when asked."));
        let thin = find(&issues, Rule::BodyThin);
        assert_eq!(thin.severity, Severity::Warn);
        assert_eq!(thin.params.as_ref().unwrap()["min"], BODY_THIN_CHARS);
    }

    #[test]
    fn a_large_file_is_body_too_large() {
        let raw = with_frontmatter(&good_frontmatter());
        let (issues, _) = run_with(Some(&raw), 64 * 1024, &[], &self_sibling(), &[], "pdf-filler");
        assert_eq!(find(&issues, Rule::BodyTooLarge).params.as_ref().unwrap()["kb"], 64);
    }

    #[test]
    fn an_unreadable_file_reports_only_that() {
        let (issues, stats) = run_with(None, 0, &[], &self_sibling(), &["claude"], "pdf-filler");
        assert_eq!(rules(&issues), vec![Rule::Unreadable]);
        assert_eq!(stats.body_chars, 0);
        // One too large to read is both.
        let (issues, _) = run_with(None, MAX_READ_BYTES + 1, &[], &[], &[], "pdf-filler");
        assert_eq!(rules(&issues), vec![Rule::Unreadable, Rule::BodyTooLarge]);
    }

    // ---- Links to files in the skill folder

    fn links(body: &str, assets: &[Asset]) -> Vec<Issue> {
        let raw = with_body(body);
        run_with(Some(&raw), raw.len() as u64, assets, &self_sibling(), &[], "pdf-filler").0
    }

    #[test]
    fn a_markdown_link_to_a_missing_file_is_broken() {
        let issues = links("Run [the helper](scripts/fill.py) when the user asks.", &[]);
        assert_eq!(find(&issues, Rule::BrokenLink).params.as_ref().unwrap()["target"], "scripts/fill.py");
    }

    #[test]
    fn a_link_to_a_file_that_is_there_is_fine() {
        let issues = links("Run [the helper](scripts/fill.py \"title\") when the user asks.", &[asset("scripts/fill.py", 400)]);
        assert!(!rules(&issues).contains(&Rule::BrokenLink));
    }

    #[test]
    fn a_backticked_path_is_checked_when_it_points_into_the_skill_folder() {
        assert!(rules(&links("Read `references/schema.json` first when needed.", &[])).contains(&Rule::BrokenLink));
        assert!(rules(&links("Run `./setup.sh` when needed.", &[])).contains(&Rule::BrokenLink));
        assert!(!rules(&links("Run `./setup.sh` when needed.", &[asset("setup.sh", 1)])).contains(&Rule::BrokenLink));
    }

    #[test]
    fn a_backticked_path_of_the_users_repo_is_not_reported() {
        let issues = links("Open `src/index.ts` and `apps/desktop/app.vue` when asked.", &[]);
        assert!(!rules(&issues).contains(&Rule::BrokenLink));
    }

    #[test]
    fn but_it_is_when_the_skill_has_that_folder() {
        let issues = links("Open `src/index.ts` when asked.", &[asset("src/other.ts", 10)]);
        assert!(rules(&issues).contains(&Rule::BrokenLink));
    }

    #[test]
    fn urls_and_anchors_are_not_files() {
        let issues = links("See [docs](https://example.com/a.html) and [top](#intro) when unsure.", &[]);
        assert!(!rules(&issues).contains(&Rule::BrokenLink));
    }

    #[test]
    fn a_link_out_of_the_skill_folder_is_reported_on_its_own() {
        let found = rules(&links("Use [shared](../common/util.py) when needed.", &[]));
        assert!(found.contains(&Rule::LinkOutsideFolder));
        assert!(!found.contains(&Rule::BrokenLink));
        assert!(rules(&links("Use `scripts/../../x.py` when needed.", &[])).contains(&Rule::LinkOutsideFolder));
    }

    #[test]
    fn an_oversized_file_is_only_info() {
        let issues = links("Nothing to link here.", &[asset("model.bin", 5 * 1024 * 1024)]);
        let oversized = find(&issues, Rule::OversizedAsset);
        assert_eq!(oversized.severity, Severity::Info);
        assert_eq!(oversized.params.as_ref().unwrap()["path"], "model.bin");
        assert_eq!(oversized.params.as_ref().unwrap()["kb"], 5120);
    }

    #[test]
    fn refs_are_collected_once_strong_wins() {
        let refs = collect_local_refs("`scripts/a.py` then [a](scripts/a.py) and `\nscripts/b.py` and ``");
        assert_eq!(refs, vec![LocalRef { target: "scripts/a.py".into(), weak: false }]);
        // A path of 121 characters in backticks isn't one.
        let long = format!("`scripts/{}.py`", "x".repeat(110));
        assert!(collect_local_refs(&long).is_empty());
    }

    // ---- Broken files

    #[test]
    fn broken_frontmatter_never_panics() {
        for raw in [
            "---\nname: Pdf Filler\ndescription: never closed",
            "---\n---\n",
            "---\n: no key\nno colon here\ndescription: |\n---\nbody",
            "---\r\nname: Pdf Filler\r\ndescription: \"Use when a PDF\"\r\n---\r\nbody",
            "---\nglobs: [\"a, b\", 'c'\ndescription: []\n---\n",
            "\u{feff}---\nname: x\n---\n",
        ] {
            let _ = run(raw);
        }
        // An unclosed block is no frontmatter at all.
        assert!(rules(&run("---\nname: Pdf Filler\ndescription: never closed").0).contains(&Rule::MissingName));
        // CRLF is read like LF.
        let (issues, _) = run("---\r\nname: Pdf Filler\r\ndescription: \"Use when a PDF form must be filled in for the user.\"\r\n---\r\nbody");
        assert!(!rules(&issues).contains(&Rule::MissingDescription));
        // A list where text is expected is no description.
        assert!(rules(&run(&with_frontmatter("---\nname: Pdf Filler\ndescription: [a, b]\n---")).0).contains(&Rule::MissingDescription));
    }

    #[test]
    fn flat_yaml_reads_lists_quotes_and_blocks() {
        let (data, body) = parse_frontmatter(
            "---\n# comment\nglobs: [\"*.ts\", 'a,b']\ntools:\n  - Bash\n  - \"Read\"\nnote: |\n  one\n  two\n\nname: 'it\\'s'\nname: x\n---\n\n\nbody",
        );
        assert_eq!(
            data,
            vec![
                ("globs".into(), FmValue::List(vec!["*.ts".into(), "a,b".into()])),
                ("tools".into(), FmValue::List(vec!["Bash".into(), "Read".into()])),
                ("note".into(), FmValue::Str("one\ntwo".into())),
                ("name".into(), FmValue::Str("x".into())),
            ]
        );
        assert_eq!(body, "body");
    }

    // ---- Disk

    struct Fixture {
        dir: PathBuf,
        db: Db,
    }

    impl Fixture {
        async fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("puppeteer-doctor-{}-{}", label, uuid::Uuid::new_v4()));
            fs::create_dir_all(dir.join("home")).unwrap();
            let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            Fixture { dir, db }
        }

        fn home(&self) -> PathBuf {
            self.dir.join("home")
        }

        fn source(&self, name: &str) -> PathBuf {
            self.dir.join("library").join(name)
        }

        /// A library skill `name` (id `id-<name>`); `skill_md` `None` leaves SKILL.md out.
        async fn skill(&self, name: &str, skill_md: Option<&[u8]>) {
            let source = self.source(name);
            fs::create_dir_all(&source).unwrap();
            if let Some(bytes) = skill_md {
                fs::write(source.join("SKILL.md"), bytes).unwrap();
            }
            sqlx::query(
                "INSERT INTO skills (id, name, description, target, source_path, updated_at)
                 VALUES (?, ?, 'd', 'claude', ?, '2026-01-01')",
            )
            .bind(format!("id-{name}"))
            .bind(name)
            .bind(source.to_string_lossy().to_string())
            .execute(&self.db)
            .await
            .unwrap();
        }

        async fn report(&self, name: &str) -> SkillDoctorReport {
            let id = format!("id-{name}");
            let mut reports = diagnose(&self.db, Some(self.home()), Some(&id)).await.unwrap();
            assert_eq!(reports.len(), 1);
            reports.remove(0)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn skill_md(name: &str, body: &str) -> Vec<u8> {
        format!("---\nname: {name}\ndescription: \"{GOOD_DESCRIPTION}\"\n---\n{body}\n").into_bytes()
    }

    #[tokio::test]
    async fn a_skill_on_disk_is_read_with_its_files() {
        let fx = Fixture::new("disk").await;
        let body = format!("{GOOD_BODY}\nRun [fill](scripts/fill.py) or [gone](scripts/gone.py).");
        fx.skill("pdf-filler", Some(&skill_md("Pdf Filler", &body))).await;
        let source = fx.source("pdf-filler");
        fs::create_dir_all(source.join("scripts/deep/deeper")).unwrap();
        fs::write(source.join("scripts/fill.py"), "x").unwrap();
        // Below the depth cap: not listed.
        fs::write(source.join("scripts/deep/too-deep.py"), "x").unwrap();
        fs::write(source.join("notes.md"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", source.join("link.txt")).unwrap();

        let report = fx.report("pdf-filler").await;
        assert_eq!((report.skill_id.as_str(), report.name.as_str()), ("id-pdf-filler", "pdf-filler"));
        assert!(report.checked_at > 0);
        assert_eq!(report.stats.asset_files, 2, "notes.md and scripts/fill.py");
        assert_eq!(rules(&report.issues), vec![Rule::BrokenLink]);
        assert_eq!(report.issues[0].params.as_ref().unwrap()["target"], "scripts/gone.py");
    }

    #[tokio::test]
    async fn broken_files_on_disk_are_unreadable() {
        let fx = Fixture::new("broken").await;
        fx.skill("missing", None).await;
        fx.skill("binary", Some(&[0xff, 0xfe, 0x00, 0x80])).await;
        let mut huge = skill_md("huge", GOOD_BODY);
        huge.resize(MAX_READ_BYTES as usize + 1, b'x');
        fx.skill("huge", Some(&huge)).await;

        assert_eq!(rules(&fx.report("missing").await.issues), vec![Rule::Unreadable]);
        let binary = fx.report("binary").await;
        assert_eq!(rules(&binary.issues), vec![Rule::Unreadable]);
        assert_eq!(binary.issues[0].params.as_ref().unwrap()["bytes"], 4);
        let huge = fx.report("huge").await;
        assert_eq!(rules(&huge.issues), vec![Rule::Unreadable, Rule::BodyTooLarge]);
        assert_eq!(huge.stats.file_bytes, MAX_READ_BYTES + 1);
    }

    #[tokio::test]
    async fn every_skill_is_checked_against_the_others() {
        let fx = Fixture::new("all").await;
        fx.skill("pdf-filler", Some(&skill_md("Pdf Filler", GOOD_BODY))).await;
        fx.skill("pdf-tools", Some(&skill_md("pdf filler", GOOD_BODY))).await;
        let reports = diagnose(&fx.db, Some(fx.home()), None).await.unwrap();
        let by_name: Vec<_> = reports.iter().map(|r| (r.name.as_str(), rules(&r.issues))).collect();
        assert_eq!(
            by_name,
            vec![
                ("pdf-filler", vec![Rule::DuplicateName]),
                ("pdf-tools", vec![Rule::NameIdMismatch, Rule::DuplicateName]),
            ]
        );
        assert_eq!(reports[0].issues[0].params.as_ref().unwrap()["other_id"], "pdf-tools");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn only_a_foreign_claude_skill_in_home_shadows() {
        let fx = Fixture::new("shadow").await;
        for name in ["mine", "ours", "codex"] {
            fx.skill(name, Some(&skill_md(name, GOOD_BODY))).await;
        }
        // The user's own `~/.claude/skills/mine`: Claude uses it instead of the library's.
        fs::create_dir_all(fx.home().join(".claude/skills/mine")).unwrap();
        // Puppeteer's own global link is the same skill.
        fs::create_dir_all(fx.home().join(".claude/skills")).unwrap();
        std::os::unix::fs::symlink(fx.source("ours"), fx.home().join(".claude/skills/ours")).unwrap();
        // Codex lists both same-named skills: nothing is hidden.
        fs::create_dir_all(fx.home().join(".agents/skills/codex")).unwrap();

        let mine = fx.report("mine").await;
        assert_eq!(rules(&mine.issues), vec![Rule::ShadowedId]);
        assert_eq!(mine.issues[0].params.as_ref().unwrap()["engine"], "claude");
        assert_eq!(fx.report("ours").await.issues, vec![]);
        assert_eq!(fx.report("codex").await.issues, vec![]);
        // Without a HOME nothing can shadow.
        assert_eq!(diagnose(&fx.db, None, Some("id-mine")).await.unwrap()[0].issues, vec![]);
    }

    #[tokio::test]
    async fn the_command_follows_its_prop_and_knows_its_skills() {
        let fx = Fixture::new("gate").await;
        fx.skill("pdf-filler", Some(&skill_md("Pdf Filler", GOOD_BODY))).await;
        assert_eq!(diagnose(&fx.db, None, Some("nope")).await.unwrap_err(), "Skill not found: nope");
        crate::commands::settings::set_setting(&fx.db, "feature.library.skill_doctor", "false").await.unwrap();
        assert_eq!(diagnose(&fx.db, None, None).await.unwrap_err(), format!("feature_disabled: {FEATURE}"));
    }
}
