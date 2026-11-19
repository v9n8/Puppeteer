//! Tiers: a (model, effort) pair, ordered model first. Model families go
//! `haiku` < `sonnet` < `opus`, efforts `low` < `medium` < `high`. Fable is a
//! family above Opus that routing never picks (U8): a Fable session's ceiling
//! counts as `opus`.
//!
//! Pure functions only, shared by the policy text, the decision records and
//! (later) escalation and the policy update.

use serde::{Deserialize, Serialize};

use crate::conductor::defaults::TASK_KINDS;

/// The task kind of work that names none, or one not in [`TASK_KINDS`].
pub const OTHER: &str = "other";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Model {
    Haiku,
    Sonnet,
    Opus,
}

impl Model {
    pub fn as_str(self) -> &'static str {
        match self {
            Model::Haiku => "haiku",
            Model::Sonnet => "sonnet",
            Model::Opus => "opus",
        }
    }

    /// The family of an alias or a Claude id (`sonnet`, `claude-sonnet-5-5`,
    /// `claude-opus-4-1[1m]`). `None`: empty (the engine picks) or unknown.
    pub fn family(model: &str) -> Option<Model> {
        let m = model.trim().to_ascii_lowercase();
        if m.contains("fable") || m.contains("opus") {
            Some(Model::Opus)
        } else if m.contains("sonnet") {
            Some(Model::Sonnet)
        } else if m.contains("haiku") {
            Some(Model::Haiku)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
}

impl Effort {
    pub const ALL: [Effort; 3] = [Effort::Low, Effort::Medium, Effort::High];

    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }

    /// One of the three routed levels; anything else (`xhigh`, `max`, junk) is
    /// `None` and is left out of comparisons.
    pub fn parse(effort: &str) -> Option<Effort> {
        Effort::ALL.into_iter().find(|e| e.as_str() == effort.trim())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tier {
    pub model: Model,
    pub effort: Effort,
}

impl Tier {
    pub const fn new(model: Model, effort: Effort) -> Self {
        Tier { model, effort }
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} / {}", self.model.as_str(), self.effort.as_str())
    }
}

/// Whether `new` sits strictly above `old`. The model decides when both sides
/// name one and they differ; else the effort, when both name one. A part
/// missing on either side is left out, so nothing known → not higher.
pub fn is_higher(new: (Option<Model>, Option<Effort>), old: (Option<Model>, Option<Effort>)) -> bool {
    if let (Some(n), Some(o)) = (new.0, old.0) {
        if n != o {
            return n > o;
        }
    }
    matches!((new.1, old.1), (Some(n), Some(o)) if n > o)
}

/// A task kind as named (`session_spawn`'s `task_kind`), or [`OTHER`].
pub fn kind_or_other(kind: Option<&str>) -> &'static str {
    let kind = kind.map(|k| k.trim().to_ascii_lowercase()).unwrap_or_default();
    TASK_KINDS.iter().find(|k| **k == kind).copied().unwrap_or(OTHER)
}

/// The task kind an `Agent` description starts with, as the policy asks:
/// `"[review] check auth diff"` → `review`. Missing or unknown → [`OTHER`].
pub fn read_kind(description: &str) -> &'static str {
    let named = description
        .trim_start()
        .strip_prefix('[')
        .and_then(|rest| rest.find(']').map(|end| &rest[..end]));
    kind_or_other(named)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Effort::*;
    use Model::*;

    #[test]
    fn families_come_from_aliases_and_ids() {
        assert_eq!(Model::family("sonnet"), Some(Sonnet));
        assert_eq!(Model::family(" claude-sonnet-5-5 "), Some(Sonnet));
        assert_eq!(Model::family("claude-opus-4-1[1m]"), Some(Opus));
        assert_eq!(Model::family("claude-haiku-5-5"), Some(Haiku));
        assert_eq!(Model::family("HAIKU"), Some(Haiku));
        // Fable is never routed to; as a ceiling it counts as Opus (U8).
        assert_eq!(Model::family("claude-fable-5-2"), Some(Opus));
        assert_eq!(Model::family(""), None);
        assert_eq!(Model::family("gpt-5-codex"), None);
    }

    #[test]
    fn the_order_is_model_first_then_effort() {
        assert!(Haiku < Sonnet && Sonnet < Opus);
        assert!(Low < Medium && Medium < High);
        assert_eq!(Effort::parse(" medium "), Some(Medium));
        for other in ["xhigh", "max", "MEDIUM", ""] {
            assert_eq!(Effort::parse(other), None, "{other}");
        }
        assert_eq!(Tier::new(Sonnet, Low).to_string(), "sonnet / low");
    }

    #[test]
    fn higher_compares_only_what_both_sides_name() {
        let t = |m, e| (Some(m), Some(e));
        assert!(is_higher(t(Opus, Low), t(Sonnet, High)), "the model decides first");
        assert!(!is_higher(t(Sonnet, High), t(Opus, Low)));
        assert!(is_higher(t(Sonnet, Medium), t(Sonnet, Low)));
        assert!(!is_higher(t(Sonnet, Low), t(Sonnet, Low)), "strictly higher");
        // One side names no model: only the efforts are compared.
        assert!(is_higher((None, Some(High)), t(Opus, Medium)));
        assert!(!is_higher((Some(Opus), None), (Some(Opus), Some(Low))));
        // Nothing comparable.
        assert!(!is_higher((Some(Opus), None), (None, Some(Low))));
        assert!(!is_higher((None, None), (None, None)));
    }

    #[test]
    fn kinds_are_read_from_the_bracket_or_fall_back_to_other() {
        assert_eq!(read_kind("[review] check auth diff"), "review");
        assert_eq!(read_kind("  [ Search ] find callers"), "search");
        assert_eq!(read_kind("[docs]"), "docs");
        for none in ["check auth diff", "[xyz] task", "[review check", "", "review: x"] {
            assert_eq!(read_kind(none), OTHER, "{none}");
        }
        assert_eq!(kind_or_other(Some("implement")), "implement");
        assert_eq!(kind_or_other(Some("xyz")), OTHER);
        assert_eq!(kind_or_other(None), OTHER);
    }
}
