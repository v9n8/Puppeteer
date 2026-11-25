//! Turns the broker's English `Ask` reason into codes + parameters the UI can
//! translate. The English text stays the source of truth (audit log, the deny
//! answer to the agent); this only reads it back. Its grammar:
//!
//! `<base>[; <az subscription>][ (as <Provider> account <D>[, as Azure account <D>])]`
//!
//! where the az subscription part can also be the whole base. Anything not
//! recognised yields no parts, and the UI falls back to the English text.

use crate::runs::broker::policy::api_call::{self, ApiCause};
use crate::runs::permission::ReasonPart;

/// Every fixed `ask("…")` text of the policy and its code. A drift test fails
/// when the policy gains a text that is not listed here.
const BASE: &[(&str, &str)] = &[
    ("unknown tool — needs confirmation", "unknownTool"),
    ("empty git command — needs confirmation", "gitEmpty"),
    ("git push writes to remote — needs confirmation", "gitPush"),
    ("unknown or unlisted git subcommand — needs confirmation", "gitUnlisted"),
    ("empty command — needs confirmation", "emptyCommand"),
    ("auth subcommand is sensitive — needs confirmation", "authSensitive"),
    ("write operation — needs confirmation", "writeOperation"),
    ("unknown or unlisted subcommand — needs confirmation", "unlistedSubcommand"),
    ("empty aws command — needs confirmation", "awsEmpty"),
    ("unrecognized aws flag before the operation — needs confirmation", "awsFlagBeforeOperation"),
    ("AWS command reveals a secret or credential — needs confirmation", "awsSecret"),
    ("aws s3 write operation — needs confirmation", "awsS3Write"),
    ("unknown or unlisted aws s3 command — needs confirmation", "awsS3Unlisted"),
    ("AWS write operation — needs confirmation", "awsWrite"),
    ("unknown or unlisted aws command — needs confirmation", "awsUnlisted"),
    ("terraform init state migration rewrites remote state — needs confirmation", "tfInitMigrate"),
    ("terraform output -json/-raw prints sensitive values — needs confirmation", "tfOutputSensitive"),
    ("terraform show -json prints sensitive values — needs confirmation", "tfShowSensitive"),
    ("terraform test may apply real infrastructure — needs confirmation", "tfTest"),
    ("terraform state pull prints the raw state (secrets) — needs confirmation", "tfStatePull"),
    ("terraform state mutation — needs confirmation", "tfStateMutation"),
    ("terraform workspace change — needs confirmation", "tfWorkspace"),
    ("terraform operation changes infrastructure or state — needs confirmation", "tfChange"),
    ("unknown or unlisted terraform command — needs confirmation", "tfUnlisted"),
    ("unrecognized az invocation — needs confirmation", "azUnrecognized"),
    ("az global flags before the command — needs confirmation", "azGlobalFlags"),
    ("`az rest` is an unrestricted backdoor — needs confirmation", "azRest"),
    ("command may reveal a secret (keys/credentials) — needs confirmation", "azSecret"),
    ("Azure write or unlisted az command — needs confirmation", "azWrite"),
];

const PROVIDERS: &[&str] = &["GitHub", "GitLab", "AWS", "Azure"];
const AZ_PREFIX: &str = "az targets subscription \"";
const AZ_MIDDLE: &str = "\", not the default subscription of Azure account ";
const NEEDS: &str = " — needs confirmation";
const TF_PREFIX: &str = "terraform config runs external code";
const TF_SUFFIX: &str = " with the linked cloud credentials — needs confirmation";

fn part(code: &str, params: &[(&str, &str)]) -> ReasonPart {
    ReasonPart {
        code: code.to_string(),
        params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    }
}

/// The reason as parts (base, then az subscription, then account(s)); empty
/// when any piece is not recognised. `argv` is the CLI's argv after the tool
/// name, used to name the cause of an `api` call.
pub fn reason_parts(reason: &str, argv: &[String]) -> Vec<ReasonPart> {
    let (rest, accounts) = match split_accounts(reason) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let (base, az) = match rest.find(AZ_PREFIX) {
        Some(0) => ("", Some(rest)),
        Some(i) if rest[..i].ends_with("; ") => (&rest[..i - 2], Some(&rest[i..])),
        _ => (rest, None),
    };
    let mut parts = Vec::new();
    if !base.is_empty() {
        match base_part(base, argv) {
            Some(p) => parts.push(p),
            None => return Vec::new(),
        }
    }
    if let Some(az) = az {
        match az_part(az) {
            Some(p) => parts.push(p),
            None => return Vec::new(),
        }
    }
    parts.extend(accounts);
    parts
}

/// Splits a trailing ` (as <Provider> account <D>[, as <Provider> account <D>])`
/// off. `None` when that suffix is there but names an unknown provider.
fn split_accounts(reason: &str) -> Option<(&str, Vec<ReasonPart>)> {
    let Some(start) = reason.strip_suffix(')').and_then(|r| r.rfind(" (as ")) else {
        return Some((reason, Vec::new()));
    };
    let inner = &reason[start + " (as ".len()..reason.len() - 1];
    let mut accounts = Vec::new();
    for one in inner.split(", as ") {
        let (provider, account) = one.split_once(" account ")?;
        if !PROVIDERS.contains(&provider) {
            return None;
        }
        accounts.push(part("asAccount", &[("provider", provider), ("account", account)]));
    }
    Some((&reason[..start], accounts))
}

fn az_part(text: &str) -> Option<ReasonPart> {
    let body = text.strip_prefix(AZ_PREFIX)?.strip_suffix(NEEDS)?;
    let (subscription, account) = body.split_once(AZ_MIDDLE)?;
    let subscription = format!("\"{subscription}\"");
    Some(part("azSubscription", &[("subscription", &subscription), ("account", account)]))
}

fn base_part(base: &str, argv: &[String]) -> Option<ReasonPart> {
    if let Some((_, code)) = BASE.iter().find(|(text, _)| *text == base) {
        return Some(part(code, &[]));
    }
    let cause = api_call::first_cause(argv).unwrap_or(ApiCause::NotApi);
    if cause.english() == base {
        let params = cause.params().into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        return Some(ReasonPart { code: cause.code().to_string(), params });
    }
    let detail = base.strip_prefix(TF_PREFIX)?.strip_suffix(TF_SUFFIX)?;
    let detail = match detail.strip_prefix(" (").and_then(|d| d.strip_suffix(')')) {
        Some(d) => d,
        None if detail.is_empty() => "",
        None => return None,
    };
    Some(part("tfExternalCode", &[("detail", detail)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn codes(reason: &str, args: &[&str]) -> Vec<String> {
        reason_parts(reason, &argv(args)).into_iter().map(|p| p.code).collect()
    }

    /// Every `ask("…")` literal in the policy source.
    fn policy_literals() -> Vec<String> {
        let src = include_str!("../policy.rs");
        let mut found = Vec::new();
        let mut rest = src;
        while let Some(i) = rest.find("ask(\"") {
            rest = &rest[i + "ask(\"".len()..];
            let end = rest.find("\")").expect("closed literal");
            found.push(rest[..end].to_string());
            rest = &rest[end..];
        }
        found
    }

    #[test]
    fn every_policy_reason_has_a_code() {
        let literals = policy_literals();
        assert!(literals.len() >= 20, "{literals:?}");
        for text in literals {
            assert!(!codes(&text, &[]).is_empty(), "no code for {text:?}");
        }
    }

    #[test]
    fn git_account_suffix() {
        let parts = reason_parts("git push writes to remote — needs confirmation (as GitHub account \"me\" (@u))", &[]);
        assert_eq!(
            parts,
            vec![
                part("gitPush", &[]),
                part("asAccount", &[("provider", "GitHub"), ("account", "\"me\" (@u)")]),
            ]
        );
    }

    #[test]
    fn cloud_account_suffixes() {
        let aws = reason_parts("AWS write operation — needs confirmation (as AWS account \"a\" (123))", &[]);
        assert_eq!(aws[1], part("asAccount", &[("provider", "AWS"), ("account", "\"a\" (123)")]));

        let azure = reason_parts("Azure write or unlisted az command — needs confirmation (as Azure account \"z\" (Sub))", &[]);
        assert_eq!(azure[1], part("asAccount", &[("provider", "Azure"), ("account", "\"z\" (Sub)")]));

        let both = reason_parts(
            "terraform operation changes infrastructure or state — needs confirmation (as AWS account \"a\", as Azure account \"z\")",
            &[],
        );
        assert_eq!(
            both,
            vec![
                part("tfChange", &[]),
                part("asAccount", &[("provider", "AWS"), ("account", "\"a\"")]),
                part("asAccount", &[("provider", "Azure"), ("account", "\"z\"")]),
            ]
        );
    }

    #[test]
    fn az_subscription_joined_or_alone() {
        let az = "az targets subscription \"S\", not the default subscription of Azure account \"L\" — needs confirmation";
        let sub = part("azSubscription", &[("subscription", "\"S\""), ("account", "\"L\"")]);
        let joined = format!("Azure write or unlisted az command — needs confirmation; {az} (as Azure account \"L\")");
        assert_eq!(
            reason_parts(&joined, &[]),
            vec![part("azWrite", &[]), sub.clone(), part("asAccount", &[("provider", "Azure"), ("account", "\"L\"")])]
        );
        assert_eq!(reason_parts(az, &[]), vec![sub]);
    }

    #[test]
    fn terraform_external_code_with_and_without_detail() {
        let with = "terraform config runs external code (local-exec in main.tf, external in data.tf) with the linked cloud credentials — needs confirmation";
        assert_eq!(
            reason_parts(with, &[]),
            vec![part("tfExternalCode", &[("detail", "local-exec in main.tf, external in data.tf")])]
        );
        let without = "terraform config runs external code with the linked cloud credentials — needs confirmation";
        assert_eq!(reason_parts(without, &[]), vec![part("tfExternalCode", &[("detail", "")])]);
    }

    #[test]
    fn api_cause_comes_from_the_argv() {
        let args = [
            "api",
            "graphql",
            "-f",
            "query=query{repository(owner:\"o\",name:\"r\"){issue(number:1078){title}}}",
            "--jq",
            ".data",
            "-c",
        ];
        let reason = format!("{} (as GitHub account \"me\")", api_call::ask_reason(&argv(&args)));
        assert_eq!(
            reason_parts(&reason, &argv(&args)),
            vec![
                part("apiUnknownFlag", &[("flag", "-c")]),
                part("asAccount", &[("provider", "GitHub"), ("account", "\"me\"")]),
            ]
        );
        let backdoor = "api/graphql is an unrestricted backdoor — needs confirmation";
        assert_eq!(codes(backdoor, &["graphql", "-f", "query=x"]), ["apiBackdoor"]);
    }

    #[test]
    fn unknown_text_has_no_parts() {
        assert!(reason_parts("something new — needs confirmation", &[]).is_empty());
        assert!(reason_parts("write operation — needs confirmation (as Bitbucket account \"x\")", &[]).is_empty());
    }
}
