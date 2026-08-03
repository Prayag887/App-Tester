use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub timestamp_ms: i64,
    pub level: String,
    pub tag: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub signature: String,
    pub category: String,
    pub title: String,
    pub count: u32,
    pub lines: Vec<LogLine>,
}

pub fn parse_logcat(line: &str) -> Option<LogLine> {
    let regex =
        Regex::new(r"^\s*(\d+(?:\.\d+)?)\s+\d+\s+\d+\s+([VDIWEAF])\s+([^:]+):\s?(.*)$").ok()?;
    let captures = regex.captures(line)?;
    Some(LogLine {
        timestamp_ms: (captures[1].parse::<f64>().ok()? * 1000.0) as i64,
        level: captures[2].into(),
        tag: captures[3].trim().into(),
        message: redact(&captures[4]),
    })
}

pub fn redact(message: &str) -> String {
    let jwt = Regex::new(r"\beyJ[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+){2}\b").expect("JWT regex");
    let bearer = Regex::new(r"(?i)\b(?:bearer|basic)\s+[A-Za-z0-9+/=_-]+").expect("auth regex");
    let userinfo = Regex::new(r"(https?://)[^/@\s]+@").expect("URL userinfo regex");
    let query = Regex::new(r"([?&][^=\s&]+)=([^&\s]+)").expect("URL query regex");
    let secret = Regex::new(r#"(?i)(\"?(?:authorization|proxy-authorization|access[_-]?token|refresh[_-]?token|session[_-]?(?:id|token)|password|passwd|secret|client[_-]?secret|api[_-]?key|cookie|token)\"?\s*[:=]\s*\"?)([^\",;&\s}]+)"#).expect("secret regex");
    let value = jwt.replace_all(message, "[REDACTED_JWT]");
    let value = bearer.replace_all(&value, "[REDACTED_AUTH]");
    let value = userinfo.replace_all(&value, "${1}[REDACTED]@");
    let value = query.replace_all(&value, "${1}=[REDACTED]");
    secret.replace_all(&value, "${1}[REDACTED]").into_owned()
}

fn classify(message: &str, level: &str) -> Option<(&'static str, &'static str)> {
    let patterns = [
        (
            "crash",
            "Crash",
            r"(?i)FATAL EXCEPTION|uncaught exception|signal \d+ \(SIG",
        ),
        (
            "anr",
            "ANR",
            r"(?i)\bANR\b|not responding|input dispatching timed out",
        ),
        (
            "network",
            "Network failure",
            r"(?i)UnknownHostException|ConnectException|SocketTimeoutException|SSLHandshakeException|CertPathValidatorException",
        ),
        (
            "parsing",
            "Response parsing failed",
            r"(?i)JsonDataException|JsonSyntaxException|SerializationException|MismatchedInputException",
        ),
    ];
    patterns
        .into_iter()
        .find_map(|(category, title, pattern)| {
            Regex::new(pattern)
                .ok()?
                .is_match(message)
                .then_some((category, title))
        })
        .or_else(|| {
            matches!(level, "W" | "E" | "F" | "A").then_some((
                if level == "W" { "warning" } else { "error" },
                if level == "W" { "Warning" } else { "Error" },
            ))
        })
}

pub struct DiagnosticBuffer {
    raw: VecDeque<LogLine>,
    diagnostics: VecDeque<Diagnostic>,
    context: VecDeque<LogLine>,
    raw_limit: usize,
    diagnostic_limit: usize,
}
impl DiagnosticBuffer {
    pub fn new(raw_limit: usize, diagnostic_limit: usize) -> Self {
        Self {
            raw: VecDeque::new(),
            diagnostics: VecDeque::new(),
            context: VecDeque::new(),
            raw_limit,
            diagnostic_limit,
        }
    }
    pub fn push(&mut self, line: LogLine) -> Option<Diagnostic> {
        self.raw.push_back(line.clone());
        while self.raw.len() > self.raw_limit {
            self.raw.pop_front();
        }
        self.context.push_back(line.clone());
        while self.context.len() > 20 {
            self.context.pop_front();
        }
        let (category, title) = classify(&line.message, &line.level)?;
        let normalized = Regex::new(r"\b(?:0x[0-9a-fA-F]+|\d{3,})\b")
            .expect("id regex")
            .replace_all(&line.message, "{id}");
        let signature = format!("{category}|{normalized}");
        if let Some(existing) = self
            .diagnostics
            .iter_mut()
            .find(|item| item.signature == signature)
        {
            existing.count += 1;
            existing.lines = self.context.iter().cloned().collect();
            return Some(existing.clone());
        }
        let item = Diagnostic {
            signature,
            category: category.into(),
            title: title.into(),
            count: 1,
            lines: self.context.iter().cloned().collect(),
        };
        self.diagnostics.push_back(item.clone());
        while self.diagnostics.len() > self.diagnostic_limit {
            self.diagnostics.pop_front();
        }
        Some(item)
    }
    pub fn raw(&self) -> Vec<LogLine> {
        self.raw.iter().cloned().collect()
    }
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_before_buffering_and_bounds() {
        let mut b = DiagnosticBuffer::new(2, 1);
        for i in 0..3 {
            b.push(parse_logcat(&format!("1721932411.123 1 2 E Api: token=value{i}")).unwrap());
        }
        assert_eq!(b.raw().len(), 2);
        assert!(b.raw().iter().all(|l| !l.message.contains("value")));
        assert_eq!(b.diagnostics().len(), 1);
    }
    #[test]
    fn redacts_auth_and_url_components() {
        let value = redact("Bearer abc123 https://user:pass@test.dev/p?safe=value&token=secret");
        assert!(!value.contains("abc123"));
        assert!(!value.contains("user:pass"));
        assert!(!value.contains("value"));
        assert!(!value.contains("secret"));
    }
}
