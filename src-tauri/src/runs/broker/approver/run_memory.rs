//! "Allow for this run": a broker `Ask` the user allowed this way lets later
//! `Ask`s of the same shape in the same run through without a prompt.
//!
//! Only read-only commands qualify, and Rust alone decides which: the webview
//! is told through the event's `allow_for_run` flag and can only answer. A
//! command that may reveal a secret never qualifies, even when it only reads.
//! The memory is in this process only and is dropped when the run ends.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use crate::commands::runs::RespondPermissionPayload;
use crate::runs::broker::audit::Origin;
use crate::runs::broker::policy::{self, api_call::{self, ApiCause}};

/// The user's answer to a broker prompt, as the approver awaits it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BrokerAnswer {
    pub allow: bool,
    /// Remember the command's shape for the rest of the run.
    pub for_run: bool,
}

impl BrokerAnswer {
    /// Only a person's answer may arm run memory; a standing rule applied by
    /// the webview never does. A phone cannot send `remember` for a broker
    /// prompt (the remote handler rejects it), so it never arms it either.
    pub fn of(payload: &RespondPermissionPayload, origin: Origin) -> Self {
        let allow = payload.decision == "allow";
        Self { allow, for_run: allow && payload.remember && origin != Origin::AutoRule }
    }
}

/// What a remembered command is matched on. Digits are masked so the same
/// query for another issue number matches; the full reason is kept so a shape
/// allowed as one account never carries over to another account or cause.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoryKey {
    tool: String,
    argv: Vec<String>,
    reason: String,
}

/// Reason codes of reads that print secrets or credentials: asking each time
/// is the only guard against the agent re-reading them in a loop.
const SECRET_CODES: &[&str] =
    &["awsSecret", "azSecret", "authSensitive", "tfOutputSensitive", "tfShowSensitive", "tfStatePull"];

const GH_GROUPS: &[&str] = &["pr", "mr", "issue", "repo", "project", "run", "ci"];
const GH_READS: &[&str] = &["list", "view", "diff", "status", "checks"];

/// The key of an `Ask`, or `None` when the command may not be remembered.
pub fn key_for(tool: &str, argv: &[String], reason: &str) -> Option<MemoryKey> {
    if !only_reads(tool, argv) {
        return None;
    }
    let parts = super::reason::reason_parts(reason, argv);
    // An unrecognised reason is not known to be harmless.
    if parts.is_empty() || parts.iter().any(|p| SECRET_CODES.contains(&p.code.as_str())) {
        return None;
    }
    Some(MemoryKey {
        tool: tool.to_string(),
        argv: argv.iter().map(|a| mask_digits(a)).collect(),
        reason: reason.to_string(),
    })
}

fn only_reads(tool: &str, argv: &[String]) -> bool {
    match tool {
        "gh" | "glab" => {
            let causes = api_call::causes(argv);
            if causes != [ApiCause::NotApi] {
                // Every other cause is about the method, body or endpoint.
                return causes.iter().all(|c| matches!(c, ApiCause::UnknownFlag(_)));
            }
            let word = |i: usize, set: &[&str]| argv.get(i).is_some_and(|w| set.contains(&w.as_str()));
            word(0, GH_GROUPS) && word(1, GH_READS)
        }
        "aws" => {
            // A flag the table does not know may have shifted the operation.
            let (path, unsure) = policy::aws_command_path(argv);
            let read = |op: &String| ["get-", "list-", "describe-"].iter().any(|p| op.starts_with(p));
            !unsure && path.get(1).is_some_and(read)
        }
        "az" => {
            // A global flag between command words may hide the real verb
            // (`az vm delete -o json list`), so the path cannot be trusted.
            let (path, interleaved) = policy::az_command_path(argv);
            if interleaved {
                return false;
            }
            // `az rest` is left out even for GET: a URL can read a secret
            // (a Key Vault secret value) and its digits are masked away.
            matches!(path.last().map(String::as_str), Some("list" | "show"))
        }
        _ => false,
    }
}

/// Every run of ASCII digits becomes one `#`.
fn mask_digits(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut in_digits = false;
    for c in arg.chars() {
        if !c.is_ascii_digit() {
            out.push(c);
        } else if !in_digits {
            out.push('#');
        }
        in_digits = c.is_ascii_digit();
    }
    out
}

/// run_id -> shapes allowed for that run.
fn store() -> &'static Mutex<HashMap<String, HashSet<MemoryKey>>> {
    static STORE: OnceLock<Mutex<HashMap<String, HashSet<MemoryKey>>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn contains(run_id: &str, key: &MemoryKey) -> bool {
    let map = store().lock().unwrap_or_else(|e| e.into_inner());
    map.get(run_id).is_some_and(|keys| keys.contains(key))
}

pub fn remember(run_id: &str, key: MemoryKey) {
    let mut map = store().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(run_id.to_string()).or_default().insert(key);
}

/// Drop everything remembered for a run (it ended or was stopped).
pub fn forget(run_id: &str) {
    store().lock().unwrap_or_else(|e| e.into_inner()).remove(run_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    const AS_ME: &str = " (as GitHub account \"me\" (@u))";

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn graphql_read(number: &str, field: &str) -> Vec<String> {
        let query = format!("query=query{{repository(owner:\"o\",name:\"r\"){{issue(number:{number}){{{field}}}}}}}");
        argv(&["api", "graphql", "-f", &query, "--jq", ".data", "-c"])
    }

    /// The policy's reason for `args`, as the broker passes it, with an account.
    fn key(tool: &str, args: &[String], account: &str) -> Option<MemoryKey> {
        let base = match tool {
            "gh" if args.first().is_some_and(|a| a == "api") => api_call::ask_reason(args),
            _ => "unknown or unlisted subcommand — needs confirmation".to_string(),
        };
        key_for(tool, args, &format!("{base}{account}"))
    }

    #[test]
    fn only_digits_differing_is_the_same_shape() {
        let a = key("gh", &graphql_read("1078", "title"), AS_ME).expect("eligible");
        assert_eq!(Some(&a), key("gh", &graphql_read("1392", "title"), AS_ME).as_ref());
        assert_eq!(Some(&a), key("gh", &graphql_read("1397", "title"), AS_ME).as_ref());
        assert_ne!(Some(&a), key("gh", &graphql_read("1078", "body"), AS_ME).as_ref());
        assert_ne!(Some(a), key("gh", &graphql_read("1078", "title"), " (as GitHub account \"other\" (@x))"));
    }

    #[test]
    fn digit_runs_become_one_mark() {
        assert_eq!(mask_digits("issue(number:1078)-v2"), "issue(number:#)-v#");
        assert_eq!(mask_digits("12"), "#");
        assert_eq!(mask_digits("abc"), "abc");
        assert_eq!(mask_digits("#1"), "##");
    }

    #[test]
    fn writes_deletes_and_file_bodies_are_never_eligible() {
        let mutation = argv(&["api", "graphql", "-f", "query=mutation{x}", "-c"]);
        let cases = [
            ("gh", argv(&["api", "-X", "DELETE", "repos/o/r/issues/comments/5"])),
            ("gh", argv(&["pr", "merge", "12"])),
            ("gh", mutation),
            ("gh", argv(&["api", "repos/o/r", "--input", "b.json", "-c"])),
            ("gh", argv(&["api", "repos/o/r/issues", "-F", "body=@notes.md"])),
            ("git", argv(&["push", "origin", "main"])),
            ("terraform", argv(&["apply"])),
            ("aws", argv(&["s3", "rm", "s3://b/k"])),
            ("aws", argv(&["ec2", "terminate-instances", "--instance-ids", "i-1"])),
            ("az", argv(&["vm", "delete", "-n", "x"])),
            ("az", argv(&["rest", "--method", "post", "--url", "u"])),
            ("az", argv(&["rest", "--url", "u"])),
        ];
        for (tool, args) in cases {
            let reason = format!("write operation — needs confirmation{AS_ME}");
            assert_eq!(key_for(tool, &args, &reason), None, "{tool} {args:?}");
        }
    }

    #[test]
    fn reads_are_eligible() {
        let reason = "unknown or unlisted aws command — needs confirmation";
        assert!(key_for("aws", &argv(&["ec2", "describe-instances", "--frobnicate"]), reason).is_some());
        assert!(key_for("aws", &argv(&["--region", "eu-west-1", "s3api", "list-buckets"]), reason).is_some());
        assert!(key("gh", &argv(&["pr", "checks", "12"]), AS_ME).is_some());
        let az = "Azure write or unlisted az command — needs confirmation";
        assert!(key_for("az", &argv(&["vm", "list", "-g", "rg"]), az).is_some());
    }

    #[test]
    fn az_rest_is_never_eligible_even_for_get() {
        let rest = "`az rest` is an unrestricted backdoor — needs confirmation";
        let vault = "https://kv1.vault.azure.net/secrets/db-pass-1?api-version=7.4";
        assert_eq!(key_for("az", &argv(&["rest", "--method", "get", "--url", vault]), rest), None);
        assert_eq!(key_for("az", &argv(&["rest", "--method", "GET", "--url", "u"]), rest), None);
        assert_eq!(key_for("az", &argv(&["rest", "-m", "get", "--url", "u"]), rest), None);
    }

    #[test]
    fn an_unsure_aws_operation_is_not_eligible() {
        let reason = "unrecognized aws flag before the operation — needs confirmation";
        assert_eq!(key_for("aws", &argv(&["--frobnicate", "ec2", "describe-instances"]), reason), None);
    }

    #[test]
    fn az_with_global_flags_between_command_words_is_not_eligible() {
        // The policy asks about the global flags before it looks for secrets
        // or verbs, so the reason alone does not reveal what these do.
        let reason = "az global flags before the command — needs confirmation";
        let keys = argv(&["-o", "json", "storage", "account", "keys", "list", "--account-name", "st1", "-g", "rg"]);
        assert_eq!(key_for("az", &keys, reason), None);
        let delete = argv(&["vm", "delete", "-o", "json", "list", "-n", "vm1", "-g", "rg", "--yes"]);
        assert_eq!(key_for("az", &delete, reason), None);
    }

    #[test]
    fn secret_reads_and_unknown_reasons_are_not_eligible() {
        let aws = argv(&["secretsmanager", "get-secret-value", "--secret-id", "x"]);
        let reason = "AWS command reveals a secret or credential — needs confirmation";
        assert_eq!(key_for("aws", &aws, reason), None);
        let az = argv(&["keyvault", "secret", "show", "--name", "x"]);
        let reason = "command may reveal a secret (keys/credentials) — needs confirmation";
        assert_eq!(key_for("az", &az, reason), None);
        assert_eq!(key_for("gh", &graphql_read("1", "title"), "something new"), None);
    }

    #[test]
    fn memory_is_per_run_and_forgotten() {
        let k = key("gh", &graphql_read("1078", "title"), AS_ME).unwrap();
        remember("memory-run-a", k.clone());
        assert!(contains("memory-run-a", &k));
        assert!(!contains("memory-run-b", &k));
        forget("memory-run-a");
        assert!(!contains("memory-run-a", &k));
    }

    fn payload(decision: &str, remember: bool) -> RespondPermissionPayload {
        serde_json::from_value(serde_json::json!({
            "run_id": "r", "request_id": "q", "decision": decision, "remember": remember,
        }))
        .unwrap()
    }

    #[test]
    fn only_a_person_allowing_with_remember_arms_the_memory() {
        let for_run = |decision, remember, origin| BrokerAnswer::of(&payload(decision, remember), origin);
        assert_eq!(for_run("allow", true, Origin::Local), BrokerAnswer { allow: true, for_run: true });
        assert_eq!(for_run("allow", true, Origin::Remote), BrokerAnswer { allow: true, for_run: true });
        assert_eq!(for_run("allow", true, Origin::AutoRule), BrokerAnswer { allow: true, for_run: false });
        assert_eq!(for_run("allow", false, Origin::Local), BrokerAnswer { allow: true, for_run: false });
        assert_eq!(for_run("deny", true, Origin::Local), BrokerAnswer::default());
    }
}
