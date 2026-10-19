use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snippet {
    pub id: String,
    pub label: String,
    pub content: String,
    pub category: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    #[serde(default = "default_true")]
    pub is_pinned: bool,
}

impl Snippet {
    pub fn new(label: String, content: String, category: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            label,
            content,
            category: category.unwrap_or_else(|| "General".to_string()),
            created_at: now,
            last_used_at: now,
            is_pinned: true,
        }
    }
}
