//! Whether a `gh api` / `glab api` call only reads, and if not, the first
//! reason why. The reason is what the user, the audit log and the agent see, so
//! it names the real cause (an unknown flag, a body from a file, ...) instead of
//! a blanket "backdoor".

/// One thing that keeps an `api` call from being a plain read. Declared in the
/// order the reason reports them: `min()` over a call's causes is the one shown.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiCause {
    /// A flag this check does not know, so it cannot tell what the flag does.
    UnknownFlag(String),
    FlagWithoutValue(String),
    /// `-F k=@file` or `--input`: the body comes from a file or stdin.
    BodyFromFile,
    MethodOverride,
    TwoEndpoints,
    NoEndpoint,
    GraphqlMutation,
    GraphqlMethod(String),
    GraphqlNoQuery,
    RestMethod(String),
    /// No `api` group (e.g. `gh graphql ...`).
    NotApi,
}

impl ApiCause {
    /// Stable code the UI translates; `reason_parts` maps the English text back to it.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownFlag(_) => "apiUnknownFlag",
            Self::FlagWithoutValue(_) => "apiFlagNoValue",
            Self::BodyFromFile => "apiBodyFromFile",
            Self::MethodOverride => "apiMethodOverride",
            Self::TwoEndpoints => "apiTwoEndpoints",
            Self::NoEndpoint => "apiNoEndpoint",
            Self::GraphqlMutation => "graphqlMutation",
            Self::GraphqlMethod(_) => "graphqlMethod",
            Self::GraphqlNoQuery => "graphqlNoQuery",
            Self::RestMethod(_) => "restMethod",
            Self::NotApi => "apiBackdoor",
        }
    }

    pub fn params(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::UnknownFlag(f) | Self::FlagWithoutValue(f) => vec![("flag", f.clone())],
            Self::GraphqlMethod(m) | Self::RestMethod(m) => vec![("method", m.clone())],
            _ => Vec::new(),
        }
    }

    /// The policy's English `Ask` reason for this cause.
    pub fn english(&self) -> String {
        let what = match self {
            Self::UnknownFlag(f) => format!("unknown flag `{f}`: can't confirm the call only reads"),
            Self::FlagWithoutValue(f) => format!("flag `{f}` has no value"),
            Self::BodyFromFile => "the request body is read from a file or stdin".into(),
            Self::MethodOverride => "a method-override header can change the HTTP method".into(),
            Self::TwoEndpoints => "more than one endpoint".into(),
            Self::NoEndpoint => "no endpoint given".into(),
            Self::GraphqlMutation => "the GraphQL query contains a mutation".into(),
            Self::GraphqlMethod(m) => format!("GraphQL sent with method {m}"),
            Self::GraphqlNoQuery => "GraphQL call without a query= field".into(),
            Self::RestMethod(m) => format!("REST method {m} is not a read"),
            Self::NotApi => "api/graphql is an unrestricted backdoor".into(),
        };
        format!("{what} — needs confirmation")
    }
}

/// Every cause that keeps `argv` (after the tool name) from being a read: a
/// REST call whose method is GET or HEAD (GET unless `-X` says otherwise or a
/// body field makes it POST), or a GraphQL call whose `query` fields hold no
/// mutation. Fail-closed: an unknown flag, a body read from a file or stdin, a
/// method override header or a second endpoint is a cause. The walk does not
/// stop at the first cause, so the reported one follows the enum order rather
/// than the token order; an unknown flag is taken as value-less to go on.
pub fn causes(argv: &[String]) -> Vec<ApiCause> {
    let start = argv.iter().position(|a| !a.starts_with('-'));
    let Some(start) = start.filter(|&i| argv[i].eq_ignore_ascii_case("api")) else {
        return vec![ApiCause::NotApi];
    };
    let rest = &argv[start + 1..];
    let mut found = Vec::new();
    let mut endpoint: Option<&str> = None;
    let mut method: Option<String> = None;
    let mut fields: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let token = rest[i].as_str();
        i += 1;
        if !token.starts_with('-') {
            if endpoint.is_some() {
                found.push(ApiCause::TwoEndpoints);
            } else {
                endpoint = Some(token);
            }
            continue;
        }
        // `--flag=value`, `-Xvalue` or `--flag value`.
        let (flag, attached) = match token.split_once('=') {
            Some((f, v)) if token.starts_with("--") => (f, Some(v)),
            _ if !token.starts_with("--") && token.len() > 2 && token.is_char_boundary(2) => {
                (&token[..2], Some(&token[2..]))
            }
            _ => (token, None),
        };
        match flag {
            "--paginate" | "--slurp" | "-i" | "--include" | "--silent" | "--verbose" => continue,
            "-X" | "--method" | "-f" | "--raw-field" | "-F" | "--field" | "-H" | "--header" | "-q" | "--jq"
            | "-t" | "--template" | "--hostname" | "--cache" | "-p" | "--preview" | "--input" => {}
            _ => {
                found.push(ApiCause::UnknownFlag(flag.chars().take(40).collect()));
                continue;
            }
        }
        let value = match attached {
            Some(v) => v,
            None if i < rest.len() => {
                i += 1;
                rest[i - 1].as_str()
            }
            None => {
                found.push(ApiCause::FlagWithoutValue(flag.to_string()));
                break;
            }
        };
        match flag {
            "--input" => found.push(ApiCause::BodyFromFile),
            "-X" | "--method" => method = Some(value.to_ascii_uppercase()),
            "-F" | "--field" if value.split_once('=').is_some_and(|(_, v)| v.starts_with('@')) => {
                found.push(ApiCause::BodyFromFile)
            }
            "-f" | "--raw-field" | "-F" | "--field" => fields.push(value),
            "-H" | "--header" if value.to_ascii_lowercase().contains("method-override") => {
                found.push(ApiCause::MethodOverride)
            }
            _ => {}
        }
    }
    let Some(endpoint) = endpoint else {
        found.push(ApiCause::NoEndpoint);
        return found;
    };
    if endpoint.trim_start_matches('/').eq_ignore_ascii_case("graphql") {
        let queries: Vec<&str> = fields.iter().filter_map(|f| f.strip_prefix("query=")).collect();
        if queries.iter().any(|q| q.to_ascii_lowercase().contains("mutation")) {
            found.push(ApiCause::GraphqlMutation);
        }
        if let Some(m) = method.filter(|m| m != "POST") {
            found.push(ApiCause::GraphqlMethod(m));
        }
        if queries.is_empty() {
            found.push(ApiCause::GraphqlNoQuery);
        }
        return found;
    }
    let method = method.unwrap_or_else(|| if fields.is_empty() { "GET" } else { "POST" }.to_string());
    if method != "GET" && method != "HEAD" {
        found.push(ApiCause::RestMethod(method));
    }
    found
}

pub fn is_read(argv: &[String]) -> bool {
    causes(argv).is_empty()
}

pub fn first_cause(argv: &[String]) -> Option<ApiCause> {
    causes(argv).into_iter().min()
}

/// The `Ask` reason for an `api` / `graphql` call that is not a read.
pub fn ask_reason(argv: &[String]) -> String {
    first_cause(argv).unwrap_or(ApiCause::NotApi).english()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(parts: &[&str]) -> Option<&'static str> {
        let argv: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
        first_cause(&argv).map(|c| c.code())
    }

    fn cause(parts: &[&str]) -> Option<ApiCause> {
        let argv: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
        first_cause(&argv)
    }

    #[test]
    fn unknown_flag_is_named() {
        assert_eq!(cause(&["api", "repos/o/r", "-c"]), Some(ApiCause::UnknownFlag("-c".into())));
    }

    #[test]
    fn flag_without_value() {
        assert_eq!(code(&["api", "repos/o/r", "-q"]), Some("apiFlagNoValue"));
    }

    #[test]
    fn body_from_file_field_or_input() {
        assert_eq!(code(&["api", "repos/o/r/issues", "-F", "body=@notes.md"]), Some("apiBodyFromFile"));
        assert_eq!(code(&["api", "repos/o/r", "--input", "b.json"]), Some("apiBodyFromFile"));
    }

    #[test]
    fn method_override_header() {
        assert_eq!(code(&["api", "repos/o/r", "-H", "X-HTTP-Method-Override: DELETE"]), Some("apiMethodOverride"));
    }

    #[test]
    fn two_endpoints() {
        assert_eq!(code(&["api", "a", "b"]), Some("apiTwoEndpoints"));
    }

    #[test]
    fn no_endpoint() {
        assert_eq!(code(&["api"]), Some("apiNoEndpoint"));
    }

    #[test]
    fn graphql_mutation() {
        assert_eq!(code(&["api", "graphql", "-f", "query=mutation{x}"]), Some("graphqlMutation"));
    }

    #[test]
    fn graphql_method() {
        assert_eq!(
            cause(&["api", "graphql", "-X", "GET", "-f", "query={ viewer { login } }"]),
            Some(ApiCause::GraphqlMethod("GET".into()))
        );
    }

    #[test]
    fn graphql_without_query() {
        assert_eq!(code(&["api", "graphql"]), Some("graphqlNoQuery"));
    }

    #[test]
    fn rest_method() {
        assert_eq!(
            cause(&["api", "-X", "DELETE", "repos/o/r/issues/comments/5"]),
            Some(ApiCause::RestMethod("DELETE".into()))
        );
    }

    #[test]
    fn no_api_group_is_the_backdoor() {
        assert_eq!(code(&["graphql", "-f", "query=x"]), Some("apiBackdoor"));
    }

    #[test]
    fn unknown_flag_wins_over_a_mutation_later_in_the_order() {
        assert_eq!(code(&["api", "graphql", "-f", "query=mutation{x}", "-c"]), Some("apiUnknownFlag"));
    }

    #[test]
    fn reported_graphql_read_names_the_unknown_flag_in_english() {
        let argv: Vec<String> = [
            "api",
            "graphql",
            "-f",
            "query=query{repository(owner:\"o\",name:\"r\"){issue(number:1078){title}}}",
            "--jq",
            ".data",
            "-c",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(first_cause(&argv), Some(ApiCause::UnknownFlag("-c".into())));
        assert!(ask_reason(&argv).contains("`-c`"), "{}", ask_reason(&argv));
        assert!(ask_reason(&argv).ends_with(" — needs confirmation"));
    }

    #[test]
    fn a_read_has_no_cause() {
        let argv: Vec<String> = ["api", "repos/o/r", "--jq", ".id"].iter().map(|s| s.to_string()).collect();
        assert!(is_read(&argv));
        assert_eq!(first_cause(&argv), None);
    }
}
