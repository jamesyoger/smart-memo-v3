use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct WorkerInput {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub text: Option<String>,
    pub categories: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoData {
    pub category: String,
    pub title: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoCard {
    pub id: usize,
    pub data: MemoData,
    pub timestamp: f64,
    pub date_str: String,
}

#[derive(Clone, PartialEq)]
pub enum AppStatus {
    NotLoaded,
    Loading,
    Ready,
    Error(String),
}

pub const STORAGE_KEY_MEMOS: &str = "ai_smart_memos";
pub const STORAGE_KEY_CATEGORIES: &str = "ai_smart_categories";