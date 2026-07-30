use regex::Regex;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentCategory {
    Crash,
    Anr,
    Error,
    Warning,
    DtoParsing,
    StrictMode,
    Database,
    WebView,
    Flutter,
    ReactNative,
    Jank,
    Memory,
    Network,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusedLogLine {
    pub timestamp_ms: i64,
    pub level: String,
    pub tag: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogIncident {
    pub id: Uuid,
    pub session_id: Uuid,
    pub category: IncidentCategory,
    pub signature: String,
    pub title: String,
    pub message: String,
    /// A user-facing explanation of what failed and why, rather than a raw log line.
    pub summary: String,
    /// The most specific causal exception found in the captured error burst.
    pub root_cause: Option<String>,
    pub first_app_frame: Option<String>,
    /// Android activity that was in the foreground while the incident occurred.
    pub foreground_activity: Option<String>,
    /// Best available code or screen location for a developer.
    pub where_occurred: String,
    /// Concise causal chain reconstructed from the log burst.
    pub how_occurred: String,
    /// Most likely underlying fault, suitable for issue reports.
    pub likely_cause: String,
    /// Deterministic steps that help reproduce the same execution path.
    pub reproduction_steps: Vec<String>,
    pub lines: Vec<FocusedLogLine>,
    pub occurrence_count: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub first_occurred_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
}

/// Parses `adb logcat -v epoch` output into a focused line. Lines outside the
/// expected format are ignored rather than being presented as actionable errors.
pub fn parse_logcat_epoch_line(line: &str) -> Option<FocusedLogLine> {
    let expression = Regex::new(
        r"^\s*(?<timestamp>\d+(?:\.\d+)?)\s+\d+\s+\d+\s+(?<level>[VDIWEAF])\s+(?<tag>[^:]+):\s?(?<message>.*)$",
    ).ok()?;
    let captures = expression.captures(line)?;
    Some(FocusedLogLine {
        timestamp_ms: (captures.name("timestamp")?.as_str().parse::<f64>().ok()? * 1000.0) as i64,
        level: captures.name("level")?.as_str().to_owned(),
        tag: captures.name("tag")?.as_str().trim().to_owned(),
        message: redact_log_message(captures.name("message")?.as_str()),
    })
}

/// Redacts authentication and analytics values before Logcat reaches storage,
/// the UI, or a copied developer report.
pub fn redact_log_message(message: &str) -> String {
    let jwt =
        Regex::new(r"\beyJ[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+){2}\b").expect("valid JWT pattern");
    let sensitive_value = Regex::new(
        r#"(?i)(\"?(?:authorization|access[_-]?token|refresh[_-]?token|firebase(?:authentication|installation)?id|sessionid|session_id|token|mobile_no|username)\"?\s*[:=]\s*\"?)([^\",\s}]+)"#,
    )
    .expect("valid sensitive log value pattern");
    let without_jwts = jwt.replace_all(message, "[REDACTED_JWT]");
    sensitive_value
        .replace_all(&without_jwts, "${1}[REDACTED]")
        .into_owned()
}

pub fn classify(message: &str) -> Option<(IncidentCategory, &'static str)> {
    let patterns = [
        (
            IncidentCategory::Crash,
            "Crash",
            r"(?i)FATAL EXCEPTION|uncaught exception|signal \d+ \(SIG",
        ),
        (
            IncidentCategory::Anr,
            "ANR",
            r"(?i)\bANR\b|not responding|input dispatching timed out",
        ),
        (
            IncidentCategory::DtoParsing,
            "DTO parsing failed",
            r#"(?i)JsonDataException|JsonSyntaxException|SerializationException|MismatchedInputException|Expected .+ but was|type ['"]?.+['"]? is not a subtype|type cast"#,
        ),
        (
            IncidentCategory::StrictMode,
            "StrictMode violation",
            r"(?i)StrictMode|DiskReadViolation|DiskWriteViolation|NetworkViolation|LeakedClosableViolation",
        ),
        (
            IncidentCategory::Database,
            "Database error",
            r"(?i)SQLiteConstraintException|SQLiteDatabaseLockedException|Room cannot verify|CursorWindow|database disk image is malformed|database or disk is full",
        ),
        (
            IncidentCategory::WebView,
            "WebView error",
            r"(?i)Render process gone|ERR_CERT_|mixed content|chromium.*uncaught",
        ),
        (
            IncidentCategory::Flutter,
            "Flutter runtime error",
            r"(?i)MissingPluginException|setState\(\) called after dispose|RenderFlex overflowed|Unhandled Exception",
        ),
        (
            IncidentCategory::ReactNative,
            "React Native error",
            r"(?i)ReactNativeJS.*Error|Hermes.*Error|native module.*error|Unable to load script",
        ),
        (
            IncidentCategory::Jank,
            "Jank detected",
            r"(?i)Skipped \d+ frames|Davey! duration=",
        ),
        (
            IncidentCategory::Network,
            "Network failure",
            r"(?i)UnknownHostException|ConnectException|SocketTimeoutException|SSLHandshakeException|CertPathValidatorException|CertificateException|Trust anchor",
        ),
    ];
    patterns.into_iter().find_map(|(category, title, pattern)| {
        Regex::new(pattern)
            .ok()?
            .is_match(message)
            .then_some((category, title))
    })
}

fn causal_exception(lines: &[FocusedLogLine]) -> Option<String> {
    lines
        .iter()
        .rev()
        .map(|line| line.message.trim())
        .find(|message| {
            (message.contains("Caused by:") || message.contains("Exception"))
                && !message.starts_with("at ")
        })
        .map(str::to_owned)
}

fn human_summary(category: IncidentCategory, root: &str, cause: Option<&str>) -> String {
    let combined = format!("{root}\n{}", cause.unwrap_or(""));
    if combined
        .to_ascii_lowercase()
        .contains("trust anchor for certification path not found")
    {
        return "The app could not establish a trusted HTTPS connection because Android does not trust the server certificate chain. This commonly happens when a proxy or self-signed certificate has not been installed or allowed for this app.".into();
    }
    match category {
        IncidentCategory::Network => "A network request failed before the app received a response. Review the root cause and the screen context to identify the affected call.".into(),
        IncidentCategory::Crash => "The app hit an unrecovered exception and crashed. The root cause below identifies the failing code path.".into(),
        IncidentCategory::Anr => "The app stopped responding long enough for Android to report an ANR. The screen context shows where the user was when it happened.".into(),
        _ => format!("The app reported: {root}"),
    }
}

fn human_title(default_title: &str, root: &str, cause: Option<&str>) -> String {
    let combined = format!("{root}\n{}", cause.unwrap_or(""));
    if combined
        .to_ascii_lowercase()
        .contains("trust anchor for certification path not found")
    {
        "TLS certificate not trusted".into()
    } else {
        default_title.into()
    }
}

fn is_tls_trust_failure(root: &str, cause: Option<&str>) -> bool {
    format!("{root}\n{}", cause.unwrap_or(""))
        .to_ascii_lowercase()
        .contains("trust anchor for certification path not found")
}

fn reproduction_steps(
    category: IncidentCategory,
    foreground_activity: Option<&str>,
    root: &str,
    cause: Option<&str>,
) -> Vec<String> {
    let mut steps = vec!["Launch the same app build with App Tester capture running.".into()];
    if let Some(activity) = foreground_activity {
        steps.push(format!("Navigate to {activity}."));
    }
    if is_tls_trust_failure(root, cause) {
        steps.push("Repeat the affected screen load or remote-configuration refresh; this failure can occur automatically during app startup.".into());
        steps.push("Verify that the App Tester CA is installed and trusted by this app's Android network-security policy, then retry the same action.".into());
        steps.push("Confirm Logcat reports CertPathValidatorException with \"Trust anchor for certification path not found\".".into());
        return steps;
    }
    steps.push(match category {
        IncidentCategory::Network => "Repeat the network action performed immediately before the failure.".into(),
        IncidentCategory::DtoParsing => "Repeat the request or screen load that parses the same response payload.".into(),
        IncidentCategory::Crash => "Repeat the last interaction shown in the evidence until the app exits.".into(),
        IncidentCategory::Anr => "Repeat the last interaction and leave the screen unchanged until Android reports it as unresponsive.".into(),
        _ => format!("Repeat the action that produced: {root}"),
    });
    steps.push("Confirm the same root cause and first application frame appear in Logcat.".into());
    steps
}

pub fn normalize_signature(
    category: IncidentCategory,
    message: &str,
    frame: Option<&str>,
) -> String {
    let ids = Regex::new(r"\b(?:0x[0-9a-fA-F]+|\d{3,}|[0-9a-fA-F]{8}-[0-9a-fA-F-]{27})\b")
        .expect("valid regex");
    format!(
        "{category:?}|{}|{}",
        ids.replace_all(message, "{id}"),
        frame.unwrap_or("")
    )
}

pub fn first_application_frame(lines: &[FocusedLogLine], package: &str) -> Option<String> {
    lines
        .iter()
        .map(|line| line.message.trim())
        .find(|line| line.starts_with("at ") && line.contains(package) && !line.contains("$Proxy"))
        .map(str::to_owned)
}

pub fn parse_incident(
    session_id: Uuid,
    package: &str,
    lines: Vec<FocusedLogLine>,
    foreground_activity: Option<String>,
) -> Option<LogIncident> {
    let (root, category, title) = lines
        .iter()
        .find_map(|line| classify(&line.message).map(|(category, title)| (line, category, title)))
        .or_else(|| {
            lines.iter().find_map(|line| match line.level.as_str() {
                "E" | "F" | "A" => Some((line, IncidentCategory::Error, "Error")),
                "W" => Some((line, IncidentCategory::Warning, "Warning")),
                _ => None,
            })
        })?;
    let frame = first_application_frame(&lines, package);
    let cause = causal_exception(&lines);
    let where_occurred = frame
        .clone()
        .or_else(|| foreground_activity.clone())
        .unwrap_or_else(|| format!("{} (Logcat)", root.tag));
    let likely_cause = cause.clone().unwrap_or_else(|| root.message.clone());
    let how_occurred = if let Some(cause) = cause.as_deref() {
        format!("{} led to {}", root.message.trim(), cause.trim())
    } else {
        format!(
            "Android reported {} while the app was active.",
            root.message.trim()
        )
    };
    let occurred_at =
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(root.timestamp_ms) * 1_000_000)
            .unwrap_or_else(|_| OffsetDateTime::now_utc());
    let reproduction_steps = reproduction_steps(
        category,
        foreground_activity.as_deref(),
        &root.message,
        cause.as_deref(),
    );
    Some(LogIncident {
        id: Uuid::new_v4(),
        session_id,
        category,
        signature: normalize_signature(category, &root.message, frame.as_deref()),
        title: human_title(title, &root.message, cause.as_deref()),
        message: root.message.clone(),
        summary: human_summary(category, &root.message, cause.as_deref()),
        root_cause: cause,
        first_app_frame: frame,
        foreground_activity,
        where_occurred,
        how_occurred,
        likely_cause,
        reproduction_steps,
        lines,
        occurrence_count: 1,
        first_occurred_at: occurred_at,
        occurred_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classifies_actionable_logs_only() {
        assert_eq!(
            classify("kotlinx.serialization.SerializationException: missing field")
                .unwrap()
                .0,
            IncidentCategory::DtoParsing
        );
        assert!(classify("GC freed 123 objects").is_none());
    }
    #[test]
    fn signatures_deduplicate_ids() {
        assert_eq!(
            normalize_signature(IncidentCategory::Crash, "user 12345", None),
            normalize_signature(IncidentCategory::Crash, "user 98765", None)
        );
    }

    #[test]
    fn parses_logcat_epoch_network_error() {
        let line = parse_logcat_epoch_line(
            "         1721932411.123 10491 10502 E TRuntime: javax.net.ssl.SSLHandshakeException: failed",
        )
        .unwrap();
        assert_eq!(line.timestamp_ms, 1_721_932_411_123);
        assert_eq!(line.tag, "TRuntime");
        assert_eq!(
            classify(&line.message).unwrap().0,
            IncidentCategory::Network
        );
    }

    #[test]
    fn redacts_sensitive_values_before_a_log_line_is_exposed() {
        let line = parse_logcat_epoch_line(
            "1721932411.123 10491 10502 D Event: firebaseAuthenticationToken=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signature mobile_no: 9860590030",
        )
        .unwrap();
        assert!(!line.message.contains("eyJhbGci"));
        assert!(!line.message.contains("9860590030"));
        assert!(line.message.contains("[REDACTED]"));
    }

    #[test]
    fn prefers_a_classified_tls_failure_over_an_earlier_generic_warning() {
        let line = |level: &str, tag: &str, message: &str| FocusedLogLine {
            timestamp_ms: 1,
            level: level.into(),
            tag: tag.into(),
            message: message.into(),
        };
        let incident = parse_incident(
            Uuid::new_v4(),
            "com.example",
            vec![
                line("W", "FCM", "FCM service not available yet (attempt 2/3)"),
                line("E", "SessionConfigFetcher", "Error failing to fetch remote configs: java.security.cert.CertPathValidatorException: Trust anchor for certification path not found."),
            ],
            Some("com.example/.MainActivity".into()),
        )
        .unwrap();
        assert_eq!(incident.title, "TLS certificate not trusted");
        assert!(incident.how_occurred.contains("CertPathValidatorException"));
        assert!(
            incident
                .reproduction_steps
                .iter()
                .any(|step| step.contains("remote-configuration refresh"))
        );
        assert!(
            !incident
                .reproduction_steps
                .iter()
                .any(|step| step.contains("attempt 2/3"))
        );
    }

    #[test]
    fn includes_unclassified_errors_and_warnings_but_drops_normal_logs() {
        let session_id = Uuid::new_v4();
        let line = |level: &str, message: &str| FocusedLogLine {
            timestamp_ms: 1,
            level: level.into(),
            tag: "Example".into(),
            message: message.into(),
        };
        assert_eq!(
            parse_incident(
                session_id,
                "dev.example",
                vec![line("E", "request rejected")],
                None
            )
            .unwrap()
            .category,
            IncidentCategory::Error
        );
        assert_eq!(
            parse_incident(
                session_id,
                "dev.example",
                vec![line("W", "slow operation")],
                None
            )
            .unwrap()
            .category,
            IncidentCategory::Warning
        );
        assert!(
            parse_incident(
                session_id,
                "dev.example",
                vec![line("I", "request completed")],
                None
            )
            .is_none()
        );
    }

    #[test]
    fn explains_untrusted_tls_certificates_in_plain_language() {
        let line = |message: &str| FocusedLogLine {
            timestamp_ms: 1,
            level: "E".into(),
            tag: "TRuntime".into(),
            message: message.into(),
        };
        let incident = parse_incident(Uuid::new_v4(), "com.example", vec![
            line("javax.net.ssl.SSLHandshakeException: Handshake failed"),
            line("Caused by: java.security.cert.CertPathValidatorException: Trust anchor for certification path not found."),
        ], Some("com.example/.CheckoutActivity".into())).unwrap();
        assert_eq!(incident.title, "TLS certificate not trusted");
        assert!(
            incident
                .summary
                .contains("does not trust the server certificate chain")
        );
        assert_eq!(
            incident.foreground_activity.as_deref(),
            Some("com.example/.CheckoutActivity")
        );
        assert_eq!(incident.where_occurred, "com.example/.CheckoutActivity");
        assert!(incident.how_occurred.contains("led to"));
        assert!(
            incident
                .reproduction_steps
                .iter()
                .any(|step| step.contains("CheckoutActivity"))
        );
    }
}
