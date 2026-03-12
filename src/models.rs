use serde::{Deserialize, Serialize};

// 🚀 날아갔던 저장소 키 상수를 다시 복구했습니다!
pub const STORAGE_KEY_MEMOS: &str = "ai_memo_vault_memos";
pub const STORAGE_KEY_CATEGORIES: &str = "ai_memo_vault_categories";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoData {
    pub category: String,
    pub content: String,
    // AI가 추출한 숫자를 담을 '금액' 필드
    #[serde(default)]
    pub amount: Option<i32>, 
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoCard {
    pub id: usize,
    pub data: MemoData,
    pub timestamp: f64,
    pub date_str: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInput {
    pub msg_type: String,
    pub text: Option<String>,
    pub categories: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppStatus {
    NotLoaded,
    Loading,
    Ready,
    Error(String),
}