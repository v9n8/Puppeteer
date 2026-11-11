//! Shared helpers for managing puppeteer-owned managed blocks inside a project's
//! `AGENTS.md` (the file Codex reads). Both rules (`kind = "rule"`) and skills
//! (`kind = "skill"`) inject/remove blocks here, keyed by `kind` + `name`, while
//! leaving all surrounding (user-authored) content untouched.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

fn block_start(kind: &str, name: &str) -> String {
    format!("<!-- puppeteer:{}:{} START -->", kind, name)
}
fn block_end(kind: &str, name: &str) -> String {
    format!("<!-- puppeteer:{}:{} END -->", kind, name)
}

/// Insert or replace the managed block for `kind`/`name` in `agents_path`,
/// preserving all other content.
pub fn upsert(agents_path: &Path, kind: &str, name: &str, body: &str) -> Result<(), String> {
    let start = block_start(kind, name);
    let end = block_end(kind, name);
    let block = format!("{}\n{}\n{}", start, body.trim_end_matches('\n'), end);

    refuse_symlink(agents_path)?;
    let existing = match fs::read_to_string(agents_path) {
        Ok(content) => content,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        // Not UTF-8 (a Shift_JIS AGENTS.md) or unreadable: writing the block over "" would wipe it.
        Err(e) => return Err(format!("Cannot read {}: {} — left it unchanged", agents_path.display(), e)),
    };

    let new_content = if let Some(s) = existing.find(&start) {
        if let Some(e_rel) = existing[s..].find(&end) {
            let e = s + e_rel + end.len();
            format!("{}{}{}", &existing[..s], block, &existing[e..])
        } else {
            // Dangling start marker — append a fresh block.
            join_block(&existing, &block)
        }
    } else {
        join_block(&existing, &block)
    };

    if let Some(parent) = agents_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(agents_path, ensure_trailing_newline(&new_content)).map_err(|e| e.to_string())
}

/// Extract the current body of the managed block for `kind`/`name`, if present.
pub fn extract(agents_path: &Path, kind: &str, name: &str) -> Option<String> {
    let content = fs::read_to_string(agents_path).ok()?;
    let start = block_start(kind, name);
    let end = block_end(kind, name);
    let s = content.find(&start)?;
    let body_start = s + start.len();
    let e_rel = content[body_start..].find(&end)?;
    Some(content[body_start..body_start + e_rel].trim_matches('\n').to_string())
}

/// Remove the managed block for `kind`/`name`, collapsing surrounding blank lines. Best effort:
/// a symlinked or unreadable AGENTS.md is left as it is.
pub fn remove(agents_path: &Path, kind: &str, name: &str) {
    if refuse_symlink(agents_path).is_err() {
        return;
    }
    let Ok(content) = fs::read_to_string(agents_path) else { return };
    let start = block_start(kind, name);
    let end = block_end(kind, name);
    let Some(s) = content.find(&start) else { return };
    let Some(e_rel) = content[s..].find(&end) else { return };
    let e = s + e_rel + end.len();

    let before = content[..s].trim_end();
    let after = content[e..].trim_start();
    let merged = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (false, true) => before.to_string(),
        (true, false) => after.to_string(),
        (false, false) => format!("{}\n\n{}", before, after),
    };

    if merged.is_empty() {
        let _ = fs::remove_file(agents_path);
    } else {
        let _ = fs::write(agents_path, ensure_trailing_newline(&merged));
    }
}

/// `content` without any managed block: what the user wrote. Unchanged when it has none; a block
/// with no END marker is kept.
pub fn strip_all(content: &str) -> String {
    let mut out = content.to_string();
    while let Some((s, e)) = next_block(&out) {
        out = format!("{}\n\n{}", out[..s].trim_end(), out[e..].trim_start()).trim().to_string();
    }
    out
}

/// `content` with every managed block's markers dropped and its text left where it was: the text
/// becomes the user's own (UX-58). A block with no END marker is kept, like [`strip_all`].
pub fn unwrap_all(content: &str) -> String {
    let mut out = String::new();
    let mut rest = content;
    while let Some((s, e)) = next_block(rest) {
        let block = &rest[s..e];
        let body_start = block.find(" START -->").map_or(0, |i| i + " START -->".len());
        let body_end = block.rfind("<!-- puppeteer:").unwrap_or(block.len());
        out.push_str(&rest[..s]);
        out.push_str(block[body_start..body_end].trim_matches(['\r', '\n']));
        rest = &rest[e..];
    }
    out.push_str(rest);
    out
}

/// `kind:name` of the managed blocks in `content`, in file order (up to one with no END marker,
/// like [`strip_all`]).
pub fn block_names(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = content;
    while let Some((s, e)) = next_block(rest) {
        let marker = &rest[s + "<!-- puppeteer:".len()..e];
        names.extend(marker.split(" START -->").next().map(str::to_string));
        rest = &rest[e..];
    }
    names
}

/// Byte range of the first managed block, from its START marker to the end of its END marker.
fn next_block(content: &str) -> Option<(usize, usize)> {
    const OPEN: &str = "<!-- puppeteer:";
    const START: &str = " START -->";
    let s = content.find(OPEN)?;
    let head = s + content[s..].find(START)?;
    let end = format!("{OPEN}{} END -->", &content[s + OPEN.len()..head]);
    let e = head + content[head..].find(&end)? + end.len();
    Some((s, e))
}

/// Writing through a symlinked AGENTS.md would put the block into whatever it points at.
fn refuse_symlink(agents_path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(agents_path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("{} is a symlink — Puppeteer won't write its block through it", agents_path.display()));
    }
    Ok(())
}

fn join_block(existing: &str, block: &str) -> String {
    if existing.trim().is_empty() {
        block.to_string()
    } else {
        format!("{}\n\n{}", existing.trim_end(), block)
    }
}

fn ensure_trailing_newline(s: &str) -> String {
    if s.ends_with('\n') {
        s.to_string()
    } else {
        format!("{}\n", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-agents-block-{}-{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_a_missing_file() {
        let agents = tmp("missing").join("AGENTS.md");
        upsert(&agents, "rule", "x", "body").unwrap();
        assert_eq!(fs::read_to_string(&agents).unwrap(), "<!-- puppeteer:rule:x START -->\nbody\n<!-- puppeteer:rule:x END -->\n");
    }

    #[test]
    fn keeps_utf8_content_around_the_block() {
        let agents = tmp("utf8").join("AGENTS.md");
        fs::write(&agents, "# Mine\n").unwrap();
        upsert(&agents, "rule", "x", "one").unwrap();
        upsert(&agents, "rule", "x", "two").unwrap();
        assert_eq!(
            fs::read_to_string(&agents).unwrap(),
            "# Mine\n\n<!-- puppeteer:rule:x START -->\ntwo\n<!-- puppeteer:rule:x END -->\n"
        );
        remove(&agents, "rule", "x");
        assert_eq!(fs::read_to_string(&agents).unwrap(), "# Mine\n");
    }

    #[test]
    fn refuses_a_file_that_is_not_utf8() {
        let agents = tmp("sjis").join("AGENTS.md");
        // "日本語" in Shift_JIS.
        let sjis = b"# \x93\xfa\x96\x7b\x8c\xea\n".to_vec();
        fs::write(&agents, &sjis).unwrap();
        assert!(upsert(&agents, "rule", "x", "body").is_err());
        assert_eq!(fs::read(&agents).unwrap(), sjis);
    }

    #[test]
    fn lists_block_names_in_order() {
        let content = "# Mine\n\n<!-- puppeteer:rule:a START -->\nx\n<!-- puppeteer:rule:a END -->\n\n\
                       <!-- puppeteer:skill:b START -->\ny\n<!-- puppeteer:skill:b END -->\n";
        assert_eq!(block_names(content), vec!["rule:a".to_string(), "skill:b".to_string()]);
        assert!(block_names("# Mine\n").is_empty());
    }

    #[test]
    fn unwrap_keeps_every_block_text_and_drops_only_the_markers() {
        let content = "# Mine\n\n<!-- puppeteer:rule:a START -->\nUse tabs.\n<!-- puppeteer:rule:a END -->\n\n\
                       Between\n\n<!-- puppeteer:skill:b START -->\n## Skill: b\nSteps\n<!-- puppeteer:skill:b END -->\n";
        assert_eq!(unwrap_all(content), "# Mine\n\nUse tabs.\n\nBetween\n\n## Skill: b\nSteps\n");
        assert!(block_names(&unwrap_all(content)).is_empty());
        // No block, or one with no END marker: left as it is.
        assert_eq!(unwrap_all("# Mine\n"), "# Mine\n");
        let open = "# Mine\n<!-- puppeteer:rule:x START -->\nmine\n";
        assert_eq!(unwrap_all(open), open);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_file() {
        let dir = tmp("link");
        let readme = dir.join("README.md");
        let block = "<!-- puppeteer:rule:x START -->\nold\n<!-- puppeteer:rule:x END -->\n";
        fs::write(&readme, format!("# Readme\n\n{block}")).unwrap();
        let agents = dir.join("proj/AGENTS.md");
        fs::create_dir_all(agents.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&readme, &agents).unwrap();

        assert!(upsert(&agents, "rule", "x", "new").is_err());
        remove(&agents, "rule", "x");
        assert_eq!(fs::read_to_string(&readme).unwrap(), format!("# Readme\n\n{block}"));
        assert!(fs::symlink_metadata(&agents).unwrap().file_type().is_symlink());
    }
}
