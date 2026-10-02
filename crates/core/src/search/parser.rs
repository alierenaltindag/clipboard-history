use crate::domain::{ClipboardEntry, EntryType};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedSearchQuery {
    pub text_query: String,
    pub entry_type: Option<EntryType>,
    pub source_app: Option<String>,
    pub is_pinned: Option<bool>,
    pub is_snippet: bool,
}

impl ParsedSearchQuery {
    pub fn parse(input: &str) -> Self {
        let mut text_parts = Vec::new();
        let mut entry_type = None;
        let mut source_app = None;
        let mut is_pinned = None;
        let mut is_snippet = false;

        for token in input.split_whitespace() {
            if let Some(val) = token.strip_prefix("type:") {
                if let Ok(et) = EntryType::from_str(val) {
                    entry_type = Some(et);
                    continue;
                }
            } else if let Some(val) = token.strip_prefix("t:") {
                if let Ok(et) = EntryType::from_str(val) {
                    entry_type = Some(et);
                    continue;
                }
            } else if let Some(val) = token.strip_prefix("app:") {
                source_app = Some(val.to_lowercase());
                continue;
            } else if let Some(val) = token.strip_prefix("source:") {
                source_app = Some(val.to_lowercase());
                continue;
            } else if token == "is:pinned" || token == "pinned:true" {
                is_pinned = Some(true);
                continue;
            } else if token == "is:unpinned" || token == "pinned:false" {
                is_pinned = Some(false);
                continue;
            } else if token == "is:snippet" || token == "type:snippet" {
                is_snippet = true;
                continue;
            }

            text_parts.push(token);
        }

        Self {
            text_query: text_parts.join(" "),
            entry_type,
            source_app,
            is_pinned,
            is_snippet,
        }
    }

    pub fn matches_entry(&self, entry: &ClipboardEntry) -> bool {
        if let Some(req_type) = self.entry_type {
            if entry.entry_type != req_type {
                return false;
            }
        }

        if let Some(req_app) = &self.source_app {
            match &entry.source_app {
                Some(app) if app.to_lowercase().contains(req_app) => {}
                _ => return false,
            }
        }

        if let Some(pinned) = self.is_pinned {
            if entry.is_pinned != pinned {
                return false;
            }
        }

        if self.is_snippet && entry.source_app.as_deref() != Some("Snippet") {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_query_parsing() {
        let parsed = ParsedSearchQuery::parse("type:code app:github is:pinned secret_token");
        assert_eq!(parsed.entry_type, Some(EntryType::Code));
        assert_eq!(parsed.source_app.as_deref(), Some("github"));
        assert_eq!(parsed.is_pinned, Some(true));
        assert_eq!(parsed.text_query, "secret_token");
        assert!(!parsed.is_snippet);

        let parsed_plain = ParsedSearchQuery::parse("just normal text");
        assert_eq!(parsed_plain.entry_type, None);
        assert_eq!(parsed_plain.source_app, None);
        assert_eq!(parsed_plain.is_pinned, None);
        assert_eq!(parsed_plain.text_query, "just normal text");

        let parsed_snippet = ParsedSearchQuery::parse("is:snippet email");
        assert!(parsed_snippet.is_snippet);
        assert_eq!(parsed_snippet.text_query, "email");
    }
}
