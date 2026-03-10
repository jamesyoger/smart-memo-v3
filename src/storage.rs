use crate::models::{MemoCard, STORAGE_KEY_CATEGORIES, STORAGE_KEY_MEMOS};

pub fn load_memos() -> Vec<MemoCard> {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(ls)) = window.local_storage() {
            if let Ok(Some(json_str)) = ls.get_item(STORAGE_KEY_MEMOS) {
                if let Ok(parsed) = serde_json::from_str::<Vec<MemoCard>>(&json_str) {
                    return parsed;
                }
            }
        }
    }
    Vec::new()
}

pub fn load_categories() -> Vec<String> {
    let default_categories = vec![
        "에러/버그".to_string(),
        "코드 스니펫".to_string(),
        "아이디어".to_string(),
        "일상/회고".to_string(),
        "기타".to_string(),
        "미분류".to_string(),
    ];
    if let Some(window) = web_sys::window() {
        if let Ok(Some(ls)) = window.local_storage() {
            if let Ok(Some(json_str)) = ls.get_item(STORAGE_KEY_CATEGORIES) {
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&json_str) {
                    if !parsed.is_empty() {
                        return parsed;
                    }
                }
            }
        }
    }
    default_categories
}

pub fn save_memos(list: &Vec<MemoCard>) {
    if let Ok(json_str) = serde_json::to_string(list) {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(ls)) = window.local_storage() {
                let _ = ls.set_item(STORAGE_KEY_MEMOS, &json_str);
            }
        }
    }
}

pub fn save_categories(list: &Vec<String>) {
    if let Ok(json_str) = serde_json::to_string(list) {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(ls)) = window.local_storage() {
                let _ = ls.set_item(STORAGE_KEY_CATEGORIES, &json_str);
            }
        }
    }
}