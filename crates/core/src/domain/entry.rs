use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Text,
    Html,
    Image,
    UriList,
    Code,
}

impl std::fmt::Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryType::Text => write!(f, "text"),
            EntryType::Html => write!(f, "html"),
            EntryType::Image => write!(f, "image"),
            EntryType::UriList => write!(f, "urilist"),
            EntryType::Code => write!(f, "code"),
        }
    }
}

impl std::str::FromStr for EntryType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(EntryType::Text),
            "html" => Ok(EntryType::Html),
            "image" => Ok(EntryType::Image),
            "urilist" | "files" => Ok(EntryType::UriList),
            "code" => Ok(EntryType::Code),
            _ => Err(format!("Unknown entry type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: String,
    pub content_hash: String,
    pub entry_type: EntryType,
    pub preview: String,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub blob_hash: Option<String>,
    pub thumbnail_blob_hash: Option<String>,
    pub mime_types: Vec<String>,
    pub size_bytes: usize,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub is_pinned: bool,
    pub source_app: Option<String>,
}

impl ClipboardEntry {
    pub fn new_text(
        text: String,
        content_hash: String,
        mime_types: Vec<String>,
        source_app: Option<String>,
    ) -> Self {
        let size_bytes = text.len();
        let preview = Self::generate_text_preview(&text);
        let entry_type = if Self::detect_is_code(&text) {
            EntryType::Code
        } else {
            EntryType::Text
        };
        let now = Utc::now();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content_hash,
            entry_type,
            preview,
            text_content: Some(text),
            html_content: None,
            blob_hash: None,
            thumbnail_blob_hash: None,
            mime_types,
            size_bytes,
            created_at: now,
            last_used_at: now,
            is_pinned: false,
            source_app,
        }
    }

    pub fn new_html(
        text: String,
        html: String,
        content_hash: String,
        mime_types: Vec<String>,
        source_app: Option<String>,
    ) -> Self {
        let size_bytes = text.len() + html.len();
        let preview = Self::generate_text_preview(&text);
        let now = Utc::now();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content_hash,
            entry_type: EntryType::Html,
            preview,
            text_content: Some(text),
            html_content: Some(html),
            blob_hash: None,
            thumbnail_blob_hash: None,
            mime_types,
            size_bytes,
            created_at: now,
            last_used_at: now,
            is_pinned: false,
            source_app,
        }
    }

    pub fn new_uri_list(
        uris: String,
        content_hash: String,
        mime_types: Vec<String>,
        source_app: Option<String>,
    ) -> Self {
        let size_bytes = uris.len();
        let count = uris.lines().filter(|l| !l.trim().is_empty()).count();
        let first_file = uris
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim_start_matches("file://");
        let preview = if count > 1 {
            format!("{} (and {} more files)", first_file, count - 1)
        } else {
            first_file.to_string()
        };
        let now = Utc::now();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content_hash,
            entry_type: EntryType::UriList,
            preview,
            text_content: Some(uris),
            html_content: None,
            blob_hash: None,
            thumbnail_blob_hash: None,
            mime_types,
            size_bytes,
            created_at: now,
            last_used_at: now,
            is_pinned: false,
            source_app,
        }
    }

    pub fn new_image(
        blob_hash: String,
        thumbnail_blob_hash: Option<String>,
        size_bytes: usize,
        dimensions: (u32, u32),
        mime_types: Vec<String>,
        source_app: Option<String>,
    ) -> Self {
        let preview = format!(
            "Image ({} × {} px, {:.1} KB)",
            dimensions.0,
            dimensions.1,
            size_bytes as f64 / 1024.0
        );
        let now = Utc::now();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content_hash: blob_hash.clone(),
            entry_type: EntryType::Image,
            preview,
            text_content: None,
            html_content: None,
            blob_hash: Some(blob_hash),
            thumbnail_blob_hash,
            mime_types,
            size_bytes,
            created_at: now,
            last_used_at: now,
            is_pinned: false,
            source_app,
        }
    }

    fn generate_text_preview(text: &str) -> String {
        let single_line = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let chars: Vec<char> = single_line.chars().collect();
        if chars.len() > 140 {
            let truncated: String = chars[..137].iter().collect();
            format!("{}...", truncated)
        } else {
            single_line
        }
    }

    fn detect_is_code(text: &str) -> bool {
        let code_indicators = [
            "fn ",
            "def ",
            "class ",
            "function ",
            "import ",
            "const ",
            "let ",
            "var ",
            "pub struct ",
            "public class ",
            "namespace ",
            "impl ",
            "#include",
            "SELECT ",
            "FROM ",
            "WHERE ",
            "<?php",
            "curl ",
            "git ",
            "docker ",
            "kubectl ",
        ];
        let has_code_keyword = code_indicators.iter().any(|k| text.contains(k));
        let has_brackets = (text.contains('{') && text.contains('}'))
            || (text.contains("->") || text.contains("=>"));
        let has_indentation = text
            .lines()
            .any(|l| l.starts_with("    ") || l.starts_with('\t'));

        (has_code_keyword && has_brackets) || (has_code_keyword && has_indentation)
    }
}
