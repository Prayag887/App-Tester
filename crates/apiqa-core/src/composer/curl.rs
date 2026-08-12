//! Parses pasted `curl` commands into composer requests.
//!
//! A best-effort importer in the spirit of Postman's: common flags map onto
//! the composer model, unknown options are skipped, and constructs we cannot
//! represent (file bodies via `-d @path`) fail loudly instead of silently
//! dropping data. Shell quoting is honored (single/double quotes, backslash
//! escapes, line continuations) but no environment expansion happens.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Serialize;

use super::{
    body::{encode_urlencoded, percent_encode},
    model::{AuthSpec, ManualBody, ManualRequest, MultipartField, SendOptions},
};
use crate::traffic::{HeaderEntry, QueryParameter};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("no URL found in the curl command")]
    MissingUrl,
    #[error("curl commands with multiple URLs are not supported")]
    MultipleUrls,
    #[error("missing value for {option}")]
    MissingValue { option: String },
    #[error("curl file bodies (-d @path) are not supported yet; use -F for file uploads")]
    FileBodyUnsupported,
    #[error("invalid --max-redirs value: {0}")]
    InvalidMaxRedirs(String),
    #[error("invalid --max-time value: {0}")]
    InvalidMaxTime(String),
}

/// The result of an import: the request plus the transport options curl
/// carried (`-k`, `-m`, `-x`, `-L`, `--max-redirs`).
#[derive(Debug, Clone, Serialize)]
pub struct CurlImport {
    pub request: ManualRequest,
    pub options: SendOptions,
}

/// Builds a runnable cURL command from the current Composer request. This
/// preserves locally entered credentials so the copied command can be run.
pub fn generate_curl_command(
    request: &ManualRequest,
    options: &SendOptions,
) -> Result<String, ParseError> {
    let mut url = url::Url::parse(&request.url).map_err(|_| ParseError::MissingUrl)?;
    for entry in &request.query {
        url.query_pairs_mut().append_pair(&entry.name, &entry.value);
    }
    if let AuthSpec::ApiKey {
        key,
        value,
        in_query: true,
    } = &request.auth
    {
        url.query_pairs_mut().append_pair(key, value);
    }

    let mut args = vec![
        "curl".to_owned(),
        "--request".to_owned(),
        request.method.clone(),
        "--url".to_owned(),
        shell_quote(url.as_str()),
    ];
    if options.follow_redirects {
        args.push("--location".to_owned());
        args.extend(["--max-redirs".to_owned(), options.max_redirects.to_string()]);
    }
    if !options.verify_tls {
        args.push("--insecure".to_owned());
    }
    if options.timeout_ms > 0 {
        args.extend([
            "--max-time".to_owned(),
            format!("{:.3}", options.timeout_ms as f64 / 1000.0),
        ]);
    }
    if let Some(proxy) = options
        .proxy_url
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        args.extend(["--proxy".to_owned(), shell_quote(proxy)]);
    }

    for header in &request.headers {
        if !matches!(
            header.name.to_ascii_lowercase().as_str(),
            "host" | "content-length" | "connection" | "proxy-connection" | "accept-encoding"
        ) {
            args.extend([
                "--header".to_owned(),
                shell_quote(&format!("{}: {}", header.name, header.value)),
            ]);
        }
    }
    match &request.auth {
        AuthSpec::Bearer { token } => args.extend([
            "--header".to_owned(),
            shell_quote(&format!("Authorization: Bearer {token}")),
        ]),
        AuthSpec::Basic { username, password } => args.extend([
            "--header".to_owned(),
            shell_quote(&format!(
                "Authorization: Basic {}",
                BASE64.encode(format!("{username}:{password}"))
            )),
        ]),
        AuthSpec::ApiKey {
            key,
            value,
            in_query: false,
        } => args.extend([
            "--header".to_owned(),
            shell_quote(&format!("{key}: {value}")),
        ]),
        AuthSpec::None | AuthSpec::ApiKey { in_query: true, .. } => {}
    }

    match &request.body {
        ManualBody::None => {}
        ManualBody::Form { fields } => {
            add_content_type_if_missing(&mut args, request, "application/x-www-form-urlencoded");
            args.extend([
                "--data-raw".to_owned(),
                shell_quote(&encode_urlencoded(fields)),
            ]);
        }
        ManualBody::Raw { media_type, text } => {
            if let Some(media_type) = media_type {
                add_content_type_if_missing(&mut args, request, media_type);
            }
            args.extend(["--data-raw".to_owned(), shell_quote(text)]);
        }
        ManualBody::Binary { bytes } => args.extend([
            "--data-binary".to_owned(),
            shell_quote(&String::from_utf8_lossy(bytes)),
        ]),
        ManualBody::Multipart { fields } => {
            for field in fields {
                let value = if let Some(path) = &field.file {
                    let mut value = format!("{}=@{path}", field.name);
                    if let Some(media_type) = &field.media_type {
                        value.push_str(&format!(";type={media_type}"));
                    }
                    value
                } else {
                    format!(
                        "{}={}",
                        field.name,
                        field.value.as_deref().unwrap_or_default()
                    )
                };
                args.extend(["--form".to_owned(), shell_quote(&value)]);
            }
        }
    }

    Ok(multiline_args(&args))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn add_content_type_if_missing(args: &mut Vec<String>, request: &ManualRequest, media_type: &str) {
    if !request
        .headers
        .iter()
        .any(|header| header.name.eq_ignore_ascii_case("content-type"))
    {
        args.extend([
            "--header".to_owned(),
            shell_quote(&format!("Content-Type: {media_type}")),
        ]);
    }
}

fn multiline_args(args: &[String]) -> String {
    let mut lines = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        if index == 0 {
            lines.push(args[index].clone());
            index += 1;
        } else if args[index].starts_with("--")
            && index + 1 < args.len()
            && !args[index + 1].starts_with("--")
        {
            lines.push(format!("  {} {}", args[index], args[index + 1]));
            index += 2;
        } else {
            lines.push(format!("  {}", args[index]));
            index += 1;
        }
    }
    lines.join(" \\\n")
}

/// Accumulated parse state, threaded through the option dispatcher.
struct CurlState {
    method: Option<String>,
    url: Option<String>,
    headers: Vec<HeaderEntry>,
    data_parts: Vec<String>,
    urlencoded: Vec<UrlencodedPart>,
    multipart: Vec<MultipartField>,
    basic: Option<(String, String)>,
    bearer: Option<String>,
    cookie: Option<String>,
    user_agent: Option<String>,
    referer: Option<String>,
    force_get: bool,
    head: bool,
    json_flag: bool,
    saw_body_flag: bool,
    options: SendOptions,
    after_double_dash: bool,
}

/// One `--data-urlencode` part: a named field or a bare encoded value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum UrlencodedPart {
    Named(String, String),
    Bare(String),
}

pub fn parse_curl(input: &str) -> Result<CurlImport, ParseError> {
    let mut state = CurlState {
        method: None,
        url: None,
        headers: Vec::new(),
        data_parts: Vec::new(),
        urlencoded: Vec::new(),
        multipart: Vec::new(),
        basic: None,
        bearer: None,
        cookie: None,
        user_agent: None,
        referer: None,
        force_get: false,
        head: false,
        json_flag: false,
        saw_body_flag: false,
        options: SendOptions::default(),
        after_double_dash: false,
    };

    let mut tokens = tokenize(input).into_iter().peekable();
    if tokens.peek().is_some_and(|token| token == "curl") {
        tokens.next();
    }
    while let Some(token) = tokens.next() {
        if state.after_double_dash {
            set_url(&mut state, token)?;
            continue;
        }
        if token == "--" {
            state.after_double_dash = true;
            continue;
        }
        if let Some((name, value)) = split_long_with_value(&token) {
            handle_option(&mut state, name, Some(value), &mut tokens)?;
            continue;
        }
        if let Some(expanded) = expand_short_flags(&token) {
            for (name, value) in expanded {
                handle_option(&mut state, &name, value.as_deref(), &mut tokens)?;
            }
            continue;
        }
        if token.starts_with('-') && token != "-" {
            handle_option(&mut state, &token, None, &mut tokens)?;
            continue;
        }
        set_url(&mut state, token)?;
    }

    let url = state.url.ok_or(ParseError::MissingUrl)?;
    let mut request = ManualRequest {
        url,
        ..Default::default()
    };

    // Authorization headers become the auth tab; `-u` wins over them.
    for header in state.headers.drain(..) {
        if header.name.eq_ignore_ascii_case("authorization") {
            match header.value.split_once(' ') {
                Some(("Bearer", token)) if !token.is_empty() => {
                    state.bearer = Some(token.to_string());
                }
                Some(("Basic", encoded)) => {
                    if let Ok(decoded) = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        encoded.trim(),
                    ) {
                        let credentials = String::from_utf8_lossy(&decoded).into_owned();
                        let (username, password) = credentials
                            .split_once(':')
                            .map_or((credentials.as_str(), ""), |(user, pass)| (user, pass));
                        state.basic = Some((username.to_string(), password.to_string()));
                    } else {
                        request.headers.push(header);
                    }
                }
                _ => request.headers.push(header),
            }
        } else {
            request.headers.push(header);
        }
    }
    request.auth = match state.basic {
        Some((username, password)) => AuthSpec::Basic { username, password },
        None => match state.bearer {
            Some(token) => AuthSpec::Bearer { token },
            None => AuthSpec::None,
        },
    };
    if let Some(cookie) = state.cookie {
        request.headers.push(HeaderEntry {
            name: "Cookie".into(),
            value: cookie,
        });
    }
    if let Some(user_agent) = state.user_agent {
        request.headers.push(HeaderEntry {
            name: "User-Agent".into(),
            value: user_agent,
        });
    }
    if let Some(referer) = state.referer {
        request.headers.push(HeaderEntry {
            name: "Referer".into(),
            value: referer,
        });
    }

    request.method = state.method.unwrap_or_else(|| {
        if state.head {
            "HEAD".into()
        } else if state.force_get {
            "GET".into()
        } else if state.saw_body_flag {
            "POST".into()
        } else {
            "GET".into()
        }
    });

    // Body resolution: multipart wins, then `-G` moves data into the query
    // string, then urlencoded fields, then raw data joined with `&`.
    if !state.multipart.is_empty() {
        request.body = ManualBody::Multipart {
            fields: state.multipart,
        };
    } else if state.force_get {
        for part in &state.data_parts {
            if part.contains('=') {
                append_query_pairs(part, &mut request.query);
            } else {
                // Bare values have no `=`; curl appends them to the query
                // string directly, which the URL row cannot express.
                append_url_query(&mut request.url, part);
            }
        }
        for part in &state.urlencoded {
            match part {
                UrlencodedPart::Named(name, value) => request.query.push(QueryParameter {
                    name: name.clone(),
                    value: value.clone(),
                }),
                UrlencodedPart::Bare(value) => {
                    append_url_query(&mut request.url, &percent_encode(value));
                }
            }
        }
        request.body = ManualBody::None;
    } else if !state.urlencoded.is_empty() {
        request.body = build_urlencoded_body(&state.urlencoded);
    } else if !state.data_parts.is_empty() {
        if state.json_flag
            && !request
                .headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("content-type"))
        {
            request.headers.push(HeaderEntry {
                name: "Content-Type".into(),
                value: "application/json".into(),
            });
        }
        request.body = ManualBody::Raw {
            media_type: state.json_flag.then(|| "application/json".to_string()),
            text: state.data_parts.join("&"),
        };
    }

    Ok(CurlImport {
        request,
        options: state.options,
    })
}

fn build_urlencoded_body(parts: &[UrlencodedPart]) -> ManualBody {
    if parts
        .iter()
        .all(|part| matches!(part, UrlencodedPart::Named(_, _)))
    {
        let fields = parts
            .iter()
            .map(|part| match part {
                UrlencodedPart::Named(name, value) => (name.clone(), value.clone()),
                UrlencodedPart::Bare(_) => unreachable!(),
            })
            .collect();
        return ManualBody::Form { fields };
    }
    // Mixed or bare parts: encode here so the wire bytes match curl exactly.
    let text = parts
        .iter()
        .map(|part| match part {
            UrlencodedPart::Named(name, value) => {
                format!("{}={}", percent_encode(name), percent_encode(value))
            }
            UrlencodedPart::Bare(value) => percent_encode(value),
        })
        .collect::<Vec<_>>()
        .join("&");
    ManualBody::Raw {
        media_type: None,
        text,
    }
}

fn append_query_pairs(pairs: &str, query: &mut Vec<QueryParameter>) {
    for pair in pairs.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        query.push(QueryParameter {
            name: name.to_string(),
            value: value.to_string(),
        });
    }
}

/// Appends a raw fragment to the URL's query string, `?` or `&` as needed.
fn append_url_query(url: &mut String, raw: &str) {
    url.push(if url.contains('?') { '&' } else { '?' });
    url.push_str(raw);
}

fn set_url(state: &mut CurlState, token: String) -> Result<(), ParseError> {
    if state.url.is_some() {
        return Err(ParseError::MultipleUrls);
    }
    state.url = Some(token);
    Ok(())
}

/// Splits `--option=value` into `("--option", Some("value"))`.
fn split_long_with_value(token: &str) -> Option<(&str, &str)> {
    if !token.starts_with("--") {
        return None;
    }
    let (name, value) = token.split_once('=')?;
    if value.is_empty() {
        return None;
    }
    Some((name, value))
}

/// Expands short-option tokens: `-XPOST` → `[("-X", Some("POST"))]`,
/// `-sSI` → `[("-s", None), ("-S", None), ("-I", None)]`. Returns `None`
/// for tokens that are not recognizable short-option bundles, leaving them
/// for the long-option or positional handling.
fn expand_short_flags(token: &str) -> Option<Vec<(String, Option<String>)>> {
    if token.starts_with("--") || !token.starts_with('-') || token.len() <= 2 {
        return None;
    }
    let mut flags = Vec::new();
    let mut chars = token[1..].chars();
    while let Some(flag) = chars.next() {
        if takes_attached_value(flag) {
            let value: String = chars.collect();
            flags.push((format!("-{flag}"), (!value.is_empty()).then_some(value)));
            return Some(flags);
        }
        if !is_bare_flag(flag) {
            return None; // unknown combined flag: leave the token untouched
        }
        flags.push((format!("-{flag}"), None));
    }
    Some(flags)
}

fn takes_attached_value(flag: char) -> bool {
    matches!(
        flag,
        'X' | 'H' | 'd' | 'F' | 'u' | 'm' | 'x' | 'b' | 'A' | 'e' | 'o' | 'D' | 'T'
    )
}

fn is_bare_flag(flag: char) -> bool {
    matches!(
        flag,
        'k' | 'L'
            | 'G'
            | 's'
            | 'S'
            | 'i'
            | 'I'
            | 'v'
            | 'g'
            | 'N'
            | 'q'
            | 'f'
            | 'O'
            | 'J'
            | 'R'
            | '0'
    )
}

type TokenStream = std::iter::Peekable<std::vec::IntoIter<String>>;

#[allow(clippy::too_many_arguments)]
fn handle_option(
    state: &mut CurlState,
    name: &str,
    value: Option<&str>,
    tokens: &mut TokenStream,
) -> Result<(), ParseError> {
    match name {
        "-X" | "--request" => state.method = Some(value_or_take(name, value, tokens)?),
        "-H" | "--header" => {
            let header = value_or_take(name, value, tokens)?;
            if let Some((header_name, header_value)) = header.split_once(':')
                && !header_name.trim().is_empty()
            {
                state.headers.push(HeaderEntry {
                    name: header_name.trim().to_string(),
                    value: header_value.trim().to_string(),
                });
            }
        }
        "-d" | "--data" | "--data-raw" | "--data-ascii" | "--data-binary" => {
            let data = value_or_take(name, value, tokens)?;
            if data.starts_with('@') {
                return Err(ParseError::FileBodyUnsupported);
            }
            state.data_parts.push(data);
            state.saw_body_flag = true;
        }
        "--data-urlencode" => {
            let data = value_or_take(name, value, tokens)?;
            if data.starts_with('@') {
                return Err(ParseError::FileBodyUnsupported);
            }
            state.urlencoded.push(parse_urlencoded_part(&data));
            state.saw_body_flag = true;
        }
        "-F" | "--form" => {
            let field = value_or_take(name, value, tokens)?;
            state.multipart.push(parse_form_field(&field));
            state.saw_body_flag = true;
        }
        "-u" | "--user" => {
            let credentials = value_or_take(name, value, tokens)?;
            let (username, password) = credentials
                .split_once(':')
                .map_or((credentials.as_str(), ""), |(user, pass)| (user, pass));
            state.basic = Some((username.to_string(), password.to_string()));
        }
        "-k" | "--insecure" => state.options.verify_tls = false,
        "-L" | "--location" => state.options.follow_redirects = true,
        "--max-redirs" => {
            let raw = value_or_take(name, value, tokens)?;
            state.options.max_redirects =
                raw.parse().map_err(|_| ParseError::InvalidMaxRedirs(raw))?;
        }
        "-m" | "--max-time" => {
            let raw = value_or_take(name, value, tokens)?;
            let seconds: f64 = raw.parse().map_err(|_| ParseError::InvalidMaxTime(raw))?;
            state.options.timeout_ms = (seconds * 1000.0) as u64;
        }
        "-x" | "--proxy" => state.options.proxy_url = Some(value_or_take(name, value, tokens)?),
        "-b" | "--cookie" => state.cookie = Some(value_or_take(name, value, tokens)?),
        "-A" | "--user-agent" => state.user_agent = Some(value_or_take(name, value, tokens)?),
        "-e" | "--referer" => state.referer = Some(value_or_take(name, value, tokens)?),
        "-G" | "--get" => state.force_get = true,
        "-I" | "--head" => state.head = true,
        "--json" => {
            state.data_parts.push(value_or_take(name, value, tokens)?);
            state.json_flag = true;
            state.saw_body_flag = true;
        }
        "--url" => set_url(state, value_or_take(name, value, tokens)?)?,
        "-s"
        | "-S"
        | "-i"
        | "-v"
        | "-g"
        | "-N"
        | "-q"
        | "-f"
        | "-O"
        | "-J"
        | "-R"
        | "-0"
        | "--globoff"
        | "--no-buffer"
        | "--disable"
        | "--fail"
        | "--fail-with-body"
        | "--compressed"
        | "--http1.0"
        | "--http1.1"
        | "--http2"
        | "--http2-prior-knowledge"
        | "--http3"
        | "--remote-name"
        | "--remote-header-name"
        | "--remote-time"
        | "--progress-bar"
        | "--silent"
        | "--show-error"
        | "--verbose"
        | "--ssl-no-revoke"
        | "--no-alpn"
        | "--no-keepalive"
        | "--no-proxy" => {}
        "-o" | "--output" | "--dump-header" | "-D" | "--cookie-jar" | "--connect-timeout"
        | "--cert" | "--key" | "--cacert" | "--capath" | "--resolve" | "--limit-rate"
        | "--range" | "--retry" | "--retry-delay" | "--speed-limit" | "--speed-time"
        | "--write-out" | "--trace" | "--trace-ascii" | "--keepalive-time" | "--oauth2-bearer"
        | "--aws-sigv4" | "-T" | "--upload-file" | "--url-query" => {}
        _ => {} // unknown options are skipped best-effort
    }
    Ok(())
}

/// The option's inline value (`--opt=value` / `-XPOST`) or the next token.
/// Only value-taking options call this, so bare flags never consume input.
fn value_or_take(
    option: &str,
    value: Option<&str>,
    tokens: &mut TokenStream,
) -> Result<String, ParseError> {
    match value {
        Some(value) => Ok(value.to_string()),
        None => tokens.next().ok_or_else(|| ParseError::MissingValue {
            option: option.to_string(),
        }),
    }
}

fn parse_urlencoded_part(data: &str) -> UrlencodedPart {
    match data.split_once('=') {
        Some((name, value)) if !name.is_empty() => {
            UrlencodedPart::Named(name.to_string(), value.to_string())
        }
        _ => UrlencodedPart::Bare(data.strip_prefix('=').unwrap_or(data).to_string()),
    }
}

fn parse_form_field(data: &str) -> MultipartField {
    let (name, rest) = data
        .split_once('=')
        .map_or(("", data), |(name, rest)| (name, rest));
    if let Some(path) = rest.strip_prefix('@') {
        let (path, media_type) = path
            .split_once(";type=")
            .map_or((path, None), |(path, media_type)| {
                (path, Some(media_type.to_string()))
            });
        return MultipartField {
            name: name.to_string(),
            value: None,
            file: Some(path.to_string()),
            media_type,
        };
    }
    MultipartField {
        name: name.to_string(),
        value: Some(rest.to_string()),
        file: None,
        media_type: None,
    }
}

/// Shell-style tokenizer: single/double quotes, backslash escapes, and
/// backslash-newline continuations. No environment or `$` expansion.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut started = false;
    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                started = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                started = true;
            }
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    if next == '\n' {
                        continue; // line continuation
                    }
                    if in_double && !matches!(next, '"' | '\\' | '$' | '`') {
                        current.push('\\');
                    }
                    current.push(next);
                    started = true;
                }
            }
            ch if ch.is_whitespace() && !in_single && !in_double => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            ch => {
                current.push(ch);
                started = true;
            }
        }
    }
    if started || in_single || in_double {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> CurlImport {
        parse_curl(input).unwrap()
    }

    #[test]
    fn simple_get_without_curl_prefix() {
        let imported = parse("https://api.test/v1/items");
        assert_eq!(imported.request.method, "GET");
        assert_eq!(imported.request.url, "https://api.test/v1/items");
        assert!(imported.request.headers.is_empty());
        assert_eq!(imported.request.body, ManualBody::None);
    }

    #[test]
    fn strips_the_curl_command_name() {
        let imported = parse("curl https://api.test/v1");
        assert_eq!(imported.request.url, "https://api.test/v1");
    }

    #[test]
    fn headers_bodies_and_bearer_auth() {
        let imported = parse(
            "curl -X POST -H 'Content-Type: application/json' \
             -H 'Authorization: Bearer tok-123' -d '{\"a\":1}' https://api.test/v1",
        );
        assert_eq!(imported.request.method, "POST");
        assert_eq!(
            imported.request.body,
            ManualBody::Raw {
                media_type: None,
                text: "{\"a\":1}".into()
            }
        );
        assert_eq!(imported.request.headers.len(), 1);
        assert_eq!(imported.request.headers[0].name, "Content-Type");
        assert_eq!(
            imported.request.auth,
            AuthSpec::Bearer {
                token: "tok-123".into()
            }
        );
    }

    #[test]
    fn basic_auth_from_user_flag_and_from_header() {
        let from_flag = parse("curl -u 'user:p@ss' https://api.test");
        assert_eq!(
            from_flag.request.auth,
            AuthSpec::Basic {
                username: "user".into(),
                password: "p@ss".into()
            }
        );
        let from_header = parse("curl -H 'Authorization: Basic dXNlcjpwYXNz' https://api.test");
        assert_eq!(
            from_header.request.auth,
            AuthSpec::Basic {
                username: "user".into(),
                password: "pass".into()
            }
        );
        assert!(from_header.request.headers.is_empty());
    }

    #[test]
    fn urlencoded_fields_become_form_body() {
        let imported = parse(
            "curl --data-urlencode 'name=John Doe' --data-urlencode 'age=30' https://api.test",
        );
        assert_eq!(
            imported.request.body,
            ManualBody::Form {
                fields: vec![
                    ("name".into(), "John Doe".into()),
                    ("age".into(), "30".into())
                ]
            }
        );
        assert_eq!(imported.request.method, "POST");
    }

    #[test]
    fn multipart_fields_with_files() {
        let imported = parse(
            "curl -F 'file=@/tmp/a.txt;type=text/plain' -F 'note=hi' https://api.test/upload",
        );
        assert_eq!(
            imported.request.body,
            ManualBody::Multipart {
                fields: vec![
                    MultipartField {
                        name: "file".into(),
                        value: None,
                        file: Some("/tmp/a.txt".into()),
                        media_type: Some("text/plain".into())
                    },
                    MultipartField {
                        name: "note".into(),
                        value: Some("hi".into()),
                        file: None,
                        media_type: None
                    },
                ]
            }
        );
    }

    #[test]
    fn transport_options_are_captured() {
        let imported =
            parse("curl -k -L --max-redirs 2 -m 5 -x http://proxy:8080 https://api.test");
        assert!(!imported.options.verify_tls);
        assert!(imported.options.follow_redirects);
        assert_eq!(imported.options.max_redirects, 2);
        assert_eq!(imported.options.timeout_ms, 5000);
        assert_eq!(
            imported.options.proxy_url.as_deref(),
            Some("http://proxy:8080")
        );
    }

    #[test]
    fn get_flag_moves_data_into_the_query_string() {
        let imported = parse("curl -G -d 'a=1' -d 'b=two words' https://api.test/search");
        assert_eq!(imported.request.method, "GET");
        assert_eq!(imported.request.body, ManualBody::None);
        assert_eq!(imported.request.query.len(), 2);
        assert_eq!(imported.request.query[0].name, "a");
        assert_eq!(imported.request.query[1].value, "two words");
    }

    #[test]
    fn attached_and_equals_forms() {
        let imported = parse(
            "curl -XPOST -H'X-Custom: v' -d'{\"x\":1}' --header='Accept: */*' --data-raw=plain https://api.test",
        );
        assert_eq!(imported.request.method, "POST");
        // curl joins multiple data flags with `&`; the parser matches that.
        assert_eq!(
            imported.request.body,
            ManualBody::Raw {
                media_type: None,
                text: "{\"x\":1}&plain".into()
            }
        );
        assert!(
            imported
                .request
                .headers
                .iter()
                .any(|h| h.name == "X-Custom" && h.value == "v")
        );
        assert!(
            imported
                .request
                .headers
                .iter()
                .any(|h| h.name == "Accept" && h.value == "*/*")
        );
    }

    #[test]
    fn quoted_values_with_escapes_and_continuations() {
        let imported = parse(
            "curl https://api.test \\\n  -d 'it'\\''s \"quoted\"' \\\n  -H \"X-Test: a\\\"b\"",
        );
        assert_eq!(
            imported.request.body,
            ManualBody::Raw {
                media_type: None,
                text: "it's \"quoted\"".into()
            }
        );
        assert!(
            imported
                .request
                .headers
                .iter()
                .any(|h| h.name == "X-Test" && h.value == "a\"b")
        );
    }

    #[test]
    fn cookie_agent_and_referer_become_headers() {
        let imported = parse(
            "curl -b 'sid=abc; theme=dark' -A 'AppTester/1.0' -e 'https://ref.test' https://api.test",
        );
        assert!(
            imported
                .request
                .headers
                .iter()
                .any(|h| h.name == "Cookie" && h.value == "sid=abc; theme=dark")
        );
        assert!(
            imported
                .request
                .headers
                .iter()
                .any(|h| h.name == "User-Agent" && h.value == "AppTester/1.0")
        );
        assert!(
            imported
                .request
                .headers
                .iter()
                .any(|h| h.name == "Referer" && h.value == "https://ref.test")
        );
    }

    #[test]
    fn json_flag_sets_content_type() {
        let imported = parse("curl --json '{\"a\":1}' https://api.test");
        assert_eq!(imported.request.method, "POST");
        assert_eq!(
            imported.request.body,
            ManualBody::Raw {
                media_type: Some("application/json".into()),
                text: "{\"a\":1}".into()
            }
        );
        assert!(
            imported
                .request
                .headers
                .iter()
                .any(|h| h.name == "Content-Type" && h.value == "application/json")
        );
    }

    #[test]
    fn head_flag_and_bare_flag_combinations() {
        let imported = parse("curl -sSI https://api.test");
        assert_eq!(imported.request.method, "HEAD");
    }

    #[test]
    fn unknown_options_are_skipped() {
        let imported = parse("curl --compressed --no-buffer -s https://api.test");
        assert_eq!(imported.request.url, "https://api.test");
        assert_eq!(imported.request.method, "GET");
    }

    #[test]
    fn multiple_data_flags_join_with_ampersand() {
        let imported = parse("curl -d 'a=1' -d 'b=2' https://api.test");
        assert_eq!(
            imported.request.body,
            ManualBody::Raw {
                media_type: None,
                text: "a=1&b=2".into()
            }
        );
    }

    #[test]
    fn bare_urlencoded_part_encodes_as_raw_body() {
        let imported = parse("curl --data-urlencode 'hello world' https://api.test");
        assert_eq!(
            imported.request.body,
            ManualBody::Raw {
                media_type: None,
                text: "hello+world".into()
            }
        );
    }

    #[test]
    fn errors_are_loud() {
        assert!(matches!(
            parse_curl("curl -H 'X: y'"),
            Err(ParseError::MissingUrl)
        ));
        assert!(matches!(
            parse_curl("curl https://a.test https://b.test"),
            Err(ParseError::MultipleUrls)
        ));
        assert!(matches!(
            parse_curl("curl -d @/etc/passwd https://a.test"),
            Err(ParseError::FileBodyUnsupported)
        ));
        assert!(matches!(
            parse_curl("curl -X"),
            Err(ParseError::MissingValue { .. })
        ));
        assert!(matches!(
            parse_curl("curl -X https://a.test"),
            Err(ParseError::MissingUrl)
        ));
        assert!(matches!(
            parse_curl("curl --max-redirs nope https://a.test"),
            Err(ParseError::InvalidMaxRedirs(_))
        ));
    }

    #[test]
    fn postman_style_full_command() {
        let imported = parse(
            "curl --location 'https://api.test/v1/items' \
             --header 'Content-Type: application/json' \
             --header 'Authorization: Bearer eyJhbGciOiJIUzI1NiJ9' \
             --data '{\"name\":\"widget\"}'",
        );
        assert_eq!(imported.request.method, "POST");
        assert_eq!(imported.request.url, "https://api.test/v1/items");
        assert!(imported.options.follow_redirects);
        assert_eq!(
            imported.request.auth,
            AuthSpec::Bearer {
                token: "eyJhbGciOiJIUzI1NiJ9".into()
            }
        );
        assert_eq!(
            imported.request.body,
            ManualBody::Raw {
                media_type: None,
                text: "{\"name\":\"widget\"}".into()
            }
        );
    }

    #[test]
    fn exports_runnable_composer_curl_with_auth_query_and_body() {
        let request = ManualRequest {
            method: "POST".into(),
            url: "https://api.test/v1/items?existing=yes".into(),
            query: vec![QueryParameter {
                name: "search".into(),
                value: "two words".into(),
            }],
            headers: vec![HeaderEntry {
                name: "X-Tenant".into(),
                value: "it's-us".into(),
            }],
            body: ManualBody::Raw {
                media_type: Some("application/json".into()),
                text: "{\"name\":\"widget\"}".into(),
            },
            auth: AuthSpec::Bearer {
                token: "secret-token".into(),
            },
        };
        let command = generate_curl_command(&request, &SendOptions::default()).unwrap();

        assert!(command.contains("--request POST"));
        assert!(command.contains("existing=yes&search=two+words"));
        assert!(command.contains("X-Tenant: it'\"'\"'s-us"));
        assert!(command.contains("Authorization: Bearer secret-token"));
        assert!(command.contains("Content-Type: application/json"));
        assert!(command.contains("--data-raw '{\"name\":\"widget\"}'"));
        assert!(command.contains("--location"));
        assert!(command.contains("--max-time 30.000"));
    }

    #[test]
    fn exports_multipart_files_and_transport_options() {
        let request = ManualRequest {
            method: "POST".into(),
            url: "https://api.test/upload".into(),
            body: ManualBody::Multipart {
                fields: vec![MultipartField {
                    name: "artifact".into(),
                    value: None,
                    file: Some("/tmp/my file.zip".into()),
                    media_type: Some("application/zip".into()),
                }],
            },
            ..ManualRequest::default()
        };
        let options = SendOptions {
            verify_tls: false,
            proxy_url: Some("http://localhost:8080".into()),
            ..SendOptions::default()
        };
        let command = generate_curl_command(&request, &options).unwrap();

        assert!(command.contains("--insecure"));
        assert!(command.contains("--proxy 'http://localhost:8080'"));
        assert!(command.contains("--form 'artifact=@/tmp/my file.zip;type=application/zip'"));
    }
}
