//! What a discovery row shows of a server's command line and URL. Another tool's config may put
//! a credential there (`--api-key=…`, `https://user:pass@host`, `?token=…`), and rows go to the
//! webview, so those parts are masked; the server is still recognisable by its command, flag
//! names, host and path. Env / header VALUES never reach a row at all (`McpSummary` holds key
//! names only). The import pipeline's own preview (`import::McpSummary` in the import dialog)
//! is not changed here.

use crate::import::McpSummary;
use crate::runs::transcript::redact;

const MASK: &str = "[redacted]";

/// Flag names (upper-cased, `-` read as `_`) whose value is a credential.
const SECRET_FLAG_WORDS: &[&str] =
    &["TOKEN", "SECRET", "PASSWORD", "PASSWD", "API_KEY", "APIKEY", "PRIVATE_KEY", "ACCESS_KEY", "CREDENTIAL", "AUTH"];

/// Flags whose value is a whole header (`X-API-Key: abc`): only the header name is kept, since
/// any header may carry a credential whatever its name.
const HEADER_FLAGS: &[&str] = &["--header", "-H"];

/// `summary` as a row shows it.
pub fn masked_summary(summary: &McpSummary) -> McpSummary {
    McpSummary {
        args: mask_args(&summary.args),
        url: summary.url.as_deref().map(mask_url),
        ..summary.clone()
    }
}

/// Whether `flag` (`--api-key`, `-t`, `--token`) names a credential.
fn is_secret_flag(flag: &str) -> bool {
    let name = flag.trim_start_matches('-').to_ascii_uppercase().replace('-', "_");
    !name.is_empty() && (SECRET_FLAG_WORDS.iter().any(|w| name.contains(w)) || name.ends_with("KEY"))
}

/// Each arg with its credentials masked: `--flag=value` and `--flag value` of a secret-named
/// flag, header values after `--header` / `-H`, URLs (user info, query, fragment), and whatever `transcript::redact` recognises by shape
/// (known key prefixes, JWTs, bearer tokens, `NAME=value` of a secret name).
fn mask_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut value_is_secret = false;
    let mut value_is_header = false;
    for arg in args {
        let masked = if value_is_header && !arg.starts_with('-') {
            mask_header(arg)
        } else if let Some((flag, header)) = arg.split_once('=').filter(|(flag, _)| HEADER_FLAGS.contains(flag)) {
            format!("{flag}={}", mask_header(header))
        } else if value_is_secret && !arg.starts_with('-') {
            MASK.to_string()
        } else if let Some((flag, _)) = arg.split_once('=').filter(|(flag, _)| flag.starts_with('-') && is_secret_flag(flag)) {
            format!("{flag}={MASK}")
        } else if arg.contains("://") {
            mask_url(arg)
        } else {
            redact(arg)
        };
        value_is_secret = arg.starts_with('-') && !arg.contains('=') && is_secret_flag(arg);
        value_is_header = HEADER_FLAGS.contains(&arg.as_str());
        out.push(masked);
    }
    out
}

/// `header` (`Name: value`) with its value masked.
fn mask_header(header: &str) -> String {
    match header.split_once(':') {
        Some((name, _)) => format!("{name}: {MASK}"),
        None => MASK.to_string(),
    }
}

/// `url` with the user info, every query value and the fragment masked; scheme, host, port and
/// path kept. Query keys stay, like env key names.
fn mask_url(url: &str) -> String {
    let (before_fragment, fragment) = match url.split_once('#') {
        Some((head, _)) => (head, Some(MASK)),
        None => (url, None),
    };
    let (base, query) = match before_fragment.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (before_fragment, None),
    };
    let mut out = match base.split_once("://") {
        Some((scheme, rest)) => {
            let authority_end = rest.find('/').unwrap_or(rest.len());
            let (authority, path) = rest.split_at(authority_end);
            match authority.rsplit_once('@') {
                Some((_, host)) => format!("{scheme}://{MASK}@{host}{path}"),
                None => base.to_string(),
            }
        }
        None => base.to_string(),
    };
    if let Some(query) = query {
        let pairs: Vec<String> = query
            .split('&')
            .map(|pair| match pair.split_once('=') {
                Some((key, _)) => format!("{key}={MASK}"),
                None if pair.is_empty() => String::new(),
                None => MASK.to_string(),
            })
            .collect();
        out.push('?');
        out.push_str(&pairs.join("&"));
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(args: &[&str], url: Option<&str>) -> McpSummary {
        McpSummary {
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: args.iter().map(|a| a.to_string()).collect(),
            url: url.map(str::to_string),
            env_keys: vec!["API_TOKEN".into()],
            header_keys: vec!["Authorization".into()],
        }
    }

    #[test]
    fn a_url_keeps_host_and_path_but_not_credentials() {
        let cases = [
            ("https://mcp.example.com/sse", "https://mcp.example.com/sse"),
            ("https://me:s3cret@mcp.example.com:8443/mcp", "https://[redacted]@mcp.example.com:8443/mcp"),
            ("https://h/mcp?token=abc123&team=x", "https://h/mcp?token=[redacted]&team=[redacted]"),
            ("https://h/mcp?abc123", "https://h/mcp?[redacted]"),
            ("https://h/cb#access_token=abc", "https://h/cb#[redacted]"),
            ("http://u@h?k=v", "http://[redacted]@h?k=[redacted]"),
        ];
        for (input, want) in cases {
            assert_eq!(mask_url(input), want, "{input}");
        }
    }

    #[test]
    fn args_hide_secret_flag_values_urls_and_known_shapes() {
        let args = [
            "-y",
            "@acme/mcp-server",
            "--api-key=sk_live_51abcdefABCDEF",
            "--token",
            "plainvalue",
            "--port",
            "8080",
            "--key",
            "secretvalue",
            "--url=https://h/x",
            "https://user:pw@h/sse?key=1",
            "GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123",
            "ghp_abcdefghijklmnopqrstuvwxyz0123",
            "--verbose",
        ];
        let out = masked_summary(&summary(&args, None)).args;
        assert_eq!(
            out,
            [
                "-y",
                "@acme/mcp-server",
                "--api-key=[redacted]",
                "--token",
                "[redacted]",
                "--port",
                "8080",
                "--key",
                "[redacted]",
                "--url=https://h/x",
                "https://[redacted]@h/sse?key=[redacted]",
                "GITHUB_TOKEN=[redacted]",
                "[redacted]",
                "--verbose",
            ]
        );
    }

    #[test]
    fn header_args_keep_the_name_but_not_the_value() {
        let args = ["--header", "X-API-Key: abc123", "-H", "Accept: json", "--header=Cookie: sid=1", "-H", "--verbose"];
        let out = masked_summary(&summary(&args, None)).args;
        assert_eq!(
            out,
            ["--header", "X-API-Key: [redacted]", "-H", "Accept: [redacted]", "--header=Cookie: [redacted]", "-H", "--verbose"]
        );
    }

    #[test]
    fn the_rest_of_the_summary_is_kept() {
        let s = masked_summary(&summary(&["serve"], Some("https://h/mcp?sig=zzz")));
        assert_eq!(s.command.as_deref(), Some("npx"));
        assert_eq!(s.args, ["serve"]);
        assert_eq!(s.url.as_deref(), Some("https://h/mcp?sig=[redacted]"));
        assert_eq!(s.env_keys, ["API_TOKEN"]);
        assert_eq!(s.header_keys, ["Authorization"]);
        assert_eq!(s.transport, "stdio");
    }
}
