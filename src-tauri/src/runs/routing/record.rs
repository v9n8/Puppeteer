//! Layer 3a: record every delegated piece of work and how it ended, from what
//! the app already receives (no model call). A row of `routing_decisions` never
//! holds a prompt or a sub-agent's output.

/// What a worker's last `Verification:` line says (QO-FR35).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Pass,
    Fail,
    None,
    /// No line, or one that does not say PASS / FAIL / NONE (a worker asked
    /// with the older free-form wording).
    Missing,
}

impl Verification {
    pub fn as_str(self) -> &'static str {
        match self {
            Verification::Pass => "pass",
            Verification::Fail => "fail",
            Verification::None => "none",
            Verification::Missing => "missing",
        }
    }
}

/// Read the outcome from a worker's final reply: the LAST line that starts with
/// `Verification:` (after leading spaces and markdown bold), then its first
/// word, any case.
pub fn parse_verification(text: &str) -> Verification {
    let Some(rest) = text.lines().rev().find_map(|line| {
        line.trim_start().trim_start_matches("**").strip_prefix("Verification:")
    }) else {
        return Verification::Missing;
    };
    // `**Verification:** PASS` closes the bold after the colon.
    let word: String = rest
        .trim_start_matches("**")
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    match word.to_ascii_uppercase().as_str() {
        "PASS" => Verification::Pass,
        "FAIL" => Verification::Fail,
        "NONE" => Verification::None,
        _ => Verification::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Verification as V;

    #[test]
    fn the_last_verification_line_decides() {
        for (text, want) in [
            ("Done.\n\nVerification: PASS — cargo test green", V::Pass),
            ("Done.\nVerification: FAIL — 2 tests red", V::Fail),
            ("Docs only.\nVerification: NONE — no code changed", V::None),
            ("Verification: pass - lint ok", V::Pass),
            ("  Verification:FAIL—build broke", V::Fail),
            ("**Verification:** PASS — all green", V::Pass),
            ("**Verification: FAIL** — 1 red", V::Fail),
            // An earlier line is overruled by the last one.
            ("Verification: FAIL — red\nfixed it\nVerification: PASS — green", V::Pass),
            // Text after the line does not hide it.
            ("Verification: NONE — docs\n\nThanks!", V::None),
        ] {
            assert_eq!(parse_verification(text), want, "{text}");
        }
    }

    #[test]
    fn no_line_or_the_old_free_form_wording_is_missing() {
        for text in [
            "",
            "All done, tests pass.",
            "Verification: ran cargo test, all green",
            "Verification: PASSED",
            "- Verification: PASS",
            "verification: PASS",
        ] {
            assert_eq!(parse_verification(text), V::Missing, "{text}");
        }
        assert_eq!(V::Missing.as_str(), "missing");
    }

    /// QO-AC48: a new worker is asked for the fixed form; one made before
    /// resumes with the wording stored on its row, not the new one.
    #[tokio::test]
    async fn a_worker_made_before_keeps_its_stored_wording() {
        let new = crate::conductor::worker_instruction(Some("You review."));
        assert!(new.ends_with(crate::conductor::WORKER_VERIFICATION));
        assert!(new.contains("`Verification: PASS — <what you ran>`"));

        let db = crate::commands::response_styles::tests::test_db().await;
        let old = "You review.\n\nBefore you finish a turn in which you changed files, run the project's tests or \
checks (test, lint, build) for the files you changed. End your reply with a `Verification:` line saying what you \
ran and the result. If no check applies, say why on that line.";
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, engine, role, conductor_run_id, system_instruction, created_at) \
             VALUES ('w', 'p1', 'session', 'claude', 'worker', 'c', ?, 'c')",
        )
        .bind(old)
        .execute(&db)
        .await
        .unwrap();
        let resumed = crate::conductor::commands::resume_framing(&db, None, "w").await.unwrap();
        assert_eq!(resumed.framing.as_deref(), Some(old));
    }
}
