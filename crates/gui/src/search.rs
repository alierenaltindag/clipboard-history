use clipboard_history_core::domain::{ClipboardEntry, EntryType};
use clipboard_history_core::search::ParsedSearchQuery;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryFilter {
    All,
    Text,
    Images,
    Files,
    Code,
    Pinned,
    Snippets,
}

#[allow(dead_code)]
pub struct FuzzySearchEngine {
    matcher: SkimMatcherV2,
}

#[allow(dead_code)]
impl FuzzySearchEngine {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn filter_entries(
        &self,
        entries: &[ClipboardEntry],
        query: &str,
        category: CategoryFilter,
    ) -> Vec<ClipboardEntry> {
        let trimmed_query = query.trim();
        let parsed = ParsedSearchQuery::parse(trimmed_query);

        let filtered_by_category: Vec<&ClipboardEntry> = entries
            .iter()
            .filter(|entry| {
                // Category bar check
                let cat_match = match category {
                    CategoryFilter::All => true,
                    CategoryFilter::Text => {
                        entry.entry_type == EntryType::Text || entry.entry_type == EntryType::Html
                    }
                    CategoryFilter::Images => entry.entry_type == EntryType::Image,
                    CategoryFilter::Files => entry.entry_type == EntryType::UriList,
                    CategoryFilter::Code => entry.entry_type == EntryType::Code,
                    CategoryFilter::Pinned => entry.is_pinned,
                    CategoryFilter::Snippets => entry.source_app.as_deref() == Some("Snippet"),
                };

                cat_match && parsed.matches_entry(entry)
            })
            .collect();

        let match_target = parsed.text_query.trim();
        if match_target.is_empty() {
            return filtered_by_category.into_iter().cloned().collect();
        }

        let mut scored: Vec<(i64, &ClipboardEntry)> = filtered_by_category
            .into_iter()
            .filter_map(|entry| {
                let mut best_score = self.matcher.fuzzy_match(&entry.preview, match_target);

                if let Some(text) = &entry.text_content {
                    // Bound the fuzzy match search candidate to the first 1000 characters
                    // to prevent UI thread freezes on multi-megabyte payloads
                    let text_candidate = text
                        .char_indices()
                        .nth(1000)
                        .map_or(text.as_str(), |(idx, _)| &text[..idx]);
                    let text_score = self.matcher.fuzzy_match(text_candidate, match_target);
                    best_score = match (best_score, text_score) {
                        (Some(s1), Some(s2)) => Some(s1.max(s2)),
                        (s1, s2) => s1.or(s2),
                    };
                }

                if let Some(src) = &entry.source_app {
                    let src_score = self.matcher.fuzzy_match(src, match_target);
                    best_score = match (best_score, src_score) {
                        (Some(s1), Some(s2)) => Some(s1.max(s2)),
                        (s1, s2) => s1.or(s2),
                    };
                }

                best_score.map(|score| {
                    // Bonus score for pinned items
                    let final_score = if entry.is_pinned { score + 1000 } else { score };
                    (final_score, entry)
                })
            })
            .collect();

        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        scored.into_iter().map(|(_, e)| e.clone()).collect()
    }
}

impl Default for FuzzySearchEngine {
    fn default() -> Self {
        Self::new()
    }
}
