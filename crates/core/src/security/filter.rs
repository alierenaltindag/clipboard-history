use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretHandlingPolicy {
    #[default]
    Reject,
    Mask,
    Allow,
}

pub struct SecretFilter {
    private_key_regex: Regex,
    credit_card_regex: Regex,
    github_token_regex: Regex,
    aws_key_regex: Regex,
    custom_patterns: Vec<Regex>,
}

static DEFAULT_FILTER: OnceLock<SecretFilter> = OnceLock::new();

impl SecretFilter {
    pub fn new(custom_regex_strings: &[String]) -> Self {
        let private_key_regex = Regex::new(
            r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----",
        )
        .unwrap();
        let credit_card_regex = Regex::new(r"\b(?:\d{4}[ -]?){3}\d{4}\b").unwrap();
        let github_token_regex =
            Regex::new(r"\b(?:ghp_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{82})\b").unwrap();
        let aws_key_regex = Regex::new(r"\b(?:AKIA[0-9A-Z]{16})\b").unwrap();

        let mut custom_patterns = Vec::new();
        for pat in custom_regex_strings {
            if let Ok(re) = Regex::new(pat) {
                custom_patterns.push(re);
            }
        }

        Self {
            private_key_regex,
            credit_card_regex,
            github_token_regex,
            aws_key_regex,
            custom_patterns,
        }
    }

    pub fn default_instance() -> &'static SecretFilter {
        DEFAULT_FILTER.get_or_init(|| Self::new(&[]))
    }

    pub fn contains_secret(&self, text: &str) -> bool {
        if self.private_key_regex.is_match(text) {
            return true;
        }
        if self.github_token_regex.is_match(text) {
            return true;
        }
        if self.aws_key_regex.is_match(text) {
            return true;
        }
        if self.credit_card_regex.is_match(text) {
            return true;
        }
        for pat in &self.custom_patterns {
            if pat.is_match(text) {
                return true;
            }
        }
        false
    }

    pub fn mask(&self, text: &str) -> String {
        let mut masked = text.to_string();
        masked = self
            .private_key_regex
            .replace_all(&masked, "[MASKED PRIVATE KEY]")
            .to_string();
        masked = self
            .github_token_regex
            .replace_all(&masked, "[MASKED TOKEN]")
            .to_string();
        masked = self
            .aws_key_regex
            .replace_all(&masked, "[MASKED AWS KEY]")
            .to_string();
        masked = self
            .credit_card_regex
            .replace_all(&masked, "[MASKED CARD]")
            .to_string();
        for pat in &self.custom_patterns {
            masked = pat.replace_all(&masked, "[MASKED SECRET]").to_string();
        }
        masked
    }
}
