use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionPolicy {
    sensitive_key_fragments: Vec<&'static str>,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            sensitive_key_fragments: vec![
                "API_KEY",
                "AUTH_TOKEN",
                "PASSWORD",
                "PRIVATE_KEY",
                "SECRET",
                "TOKEN",
            ],
        }
    }
}

impl RedactionPolicy {
    pub fn redact(&self, input: &str) -> String {
        input
            .lines()
            .map(|line| self.redact_line(line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn redact_line(&self, line: &str) -> String {
        let Some((key, _value)) = line.split_once('=') else {
            return line.to_string();
        };
        let normalized_key = key
            .trim()
            .trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace())
            .to_ascii_uppercase();
        if self
            .sensitive_key_fragments
            .iter()
            .any(|fragment| normalized_key.contains(fragment))
        {
            format!("{}=[REDACTED]", key.trim_end())
        } else {
            line.to_string()
        }
    }
}

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}
