use leptos::prelude::*;
use leptos::html::{Input, Textarea};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Worker};

use crate::models::{AppStatus, MemoCard, MemoData, WorkerInput};
use crate::storage::{load_categories, load_memos, save_categories, save_memos};

#[derive(Copy, Clone)]
pub struct AppStore {
    pub app_status: ReadSignal<AppStatus>,
    pub set_app_status: WriteSignal<AppStatus>,
    pub input_text: ReadSignal<String>,
    pub set_input_text: WriteSignal<String>,
    pub is_generating: ReadSignal<bool>,
    pub set_is_generating: WriteSignal<bool>,
    pub search_query: ReadSignal<String>,
    pub set_search_query: WriteSignal<String>,
    pub is_ai_searching: ReadSignal<bool>,
    pub set_is_ai_searching: WriteSignal<bool>,
    pub ai_search_results: ReadSignal<Option<Vec<usize>>>,
    pub set_ai_search_results: WriteSignal<Option<Vec<usize>>>,
    pub ai_search_status: ReadSignal<String>,
    pub set_ai_search_status: WriteSignal<String>,
    
    pub target_memo_id: ReadSignal<Option<usize>>,
    pub set_target_memo_id: WriteSignal<Option<usize>>,
    pub show_category_manager: ReadSignal<bool>,
    pub set_show_category_manager: WriteSignal<bool>,
    pub new_category_name: ReadSignal<String>,
    pub set_new_category_name: WriteSignal<String>,
    pub memo_list: ReadSignal<Vec<MemoCard>>,
    pub set_memo_list: WriteSignal<Vec<MemoCard>>,
    pub category_list: ReadSignal<Vec<String>>,
    pub set_category_list: WriteSignal<Vec<String>>,
    pub worker_status_msg: ReadSignal<String>,
    pub worker_progress: ReadSignal<f64>,
    pub worker_store: StoredValue<Worker>,
    pub memo_input_ref: NodeRef<Textarea>,
    pub search_input_ref: NodeRef<Input>,
}

impl AppStore {
    pub fn new() -> Self {
        let (app_status, set_app_status) = signal(AppStatus::NotLoaded);
        let (input_text, set_input_text) = signal(String::new());
        let (is_generating, set_is_generating) = signal(false);
        let (search_query, set_search_query) = signal(String::new());
        let (is_ai_searching, set_is_ai_searching) = signal(false);
        let (ai_search_results, set_ai_search_results) = signal::<Option<Vec<usize>>>(None);
        let (ai_search_status, set_ai_search_status) = signal(String::new());
        
        let (target_memo_id, set_target_memo_id) = signal::<Option<usize>>(None);
        let (show_category_manager, set_show_category_manager) = signal(false);
        let (new_category_name, set_new_category_name) = signal(String::new());

        let (memo_list, set_memo_list) = signal(load_memos());
        let (category_list, set_category_list) = signal(load_categories());

        Effect::new(move |_| save_memos(&memo_list.get()));
        Effect::new(move |_| save_categories(&category_list.get()));

        let (worker_status_msg, set_worker_status_msg) = signal(String::new());
        let (worker_progress, set_worker_progress) = signal(0.0f64);

        let memo_input_ref = NodeRef::<Textarea>::new();
        let search_input_ref = NodeRef::<Input>::new();

        let worker = {
            let options = web_sys::WorkerOptions::new();
            options.set_type(web_sys::WorkerType::Module);
            // 버전 업그레이드!
            Worker::new_with_options("ai_worker.js?v=25_money", &options).expect("워커 생성 실패!")
        };
        let worker_store = StoredValue::new(worker.clone());

        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(js_obj) = e.data().dyn_into::<js_sys::Object>() {
                let msg_type = js_sys::Reflect::get(&js_obj, &JsValue::from_str("type")).unwrap_or(JsValue::NULL).as_string().unwrap_or_default();
                let text = js_sys::Reflect::get(&js_obj, &JsValue::from_str("text")).unwrap_or(JsValue::NULL).as_string().unwrap_or_default();
                let progress = js_sys::Reflect::get(&js_obj, &JsValue::from_str("progress")).unwrap_or(JsValue::from_f64(0.0)).as_f64().unwrap_or(0.0);
                let result_payload = js_sys::Reflect::get(&js_obj, &JsValue::from_str("result")).unwrap_or(JsValue::NULL).as_string().unwrap_or_default();

                match msg_type.as_str() {
                    "STATUS" => { set_worker_status_msg.set(text); set_worker_progress.set(progress); },
                    "READY" => set_app_status.set(AppStatus::Ready),
                    "TOKEN" => {},
                    "TOKEN_SEARCH" => set_ai_search_status.set(text),
                    "DONE_CLASSIFY" => {
                        let raw_json = result_payload;
                        let clean_json = if let (Some(start), Some(end)) = (raw_json.find('{'), raw_json.rfind('}')) { raw_json[start..=end].to_string() } else { raw_json.clone() };
                        let now = js_sys::Date::new_0();
                        let timestamp = now.get_time();
                        let date_str = format!("{:04}-{:02}-{:02} {:02}:{:02}", now.get_full_year(), now.get_month() + 1, now.get_date(), now.get_hours(), now.get_minutes());

                        match serde_json::from_str::<MemoData>(&clean_json) {
                            Ok(mut parsed_data) => {
                                if parsed_data.content.trim().is_empty() || parsed_data.content.len() < 5 { parsed_data.content = input_text.get_untracked(); }
                                set_memo_list.update(|list| {
                                    if let Some(target_id) = target_memo_id.get_untracked() {
                                        if let Some(memo) = list.iter_mut().find(|m| m.id == target_id) {
                                            memo.data = parsed_data; memo.timestamp = timestamp; memo.date_str = date_str.clone();
                                        }
                                    } else {
                                        let new_id = list.iter().map(|m| m.id).max().unwrap_or(0) + 1;
                                        list.push(MemoCard { id: new_id, data: parsed_data, timestamp, date_str });
                                    }
                                });
                            },
                            Err(_) => {
                                set_memo_list.update(|list| {
                                    let new_id = list.iter().map(|m| m.id).max().unwrap_or(0) + 1;
                                    // 🚀 Fallback에도 amount: None 추가
                                    list.push(MemoCard { id: new_id, data: MemoData { category: "미분류".to_string(), content: input_text.get_untracked(), amount: None }, timestamp, date_str });
                                });
                            }
                        }
                        
                        set_is_generating.set(false);
                        if target_memo_id.get_untracked().is_none() {
                            set_input_text.set(String::new());
                            if let Some(ta) = memo_input_ref.get() {
                                ta.set_value("");
                                let _ = web_sys::HtmlElement::style(&ta).set_property("height", "auto");
                            }
                        }
                        set_target_memo_id.set(None); 
                    },
                    "DONE_SEARCH" => {
                        let mut found_ids = Vec::new();
                        if let Ok(ids) = serde_json::from_str::<Vec<usize>>(&result_payload) { found_ids = ids; }
                        set_ai_search_results.set(Some(found_ids));
                        set_is_ai_searching.set(false);
                        set_ai_search_status.set(String::new());
                    },
                    "ERROR" => {
                        web_sys::console::error_1(&JsValue::from_str(&format!("🔥 에러: {}", text)));
                        set_app_status.set(AppStatus::Error(text));
                        set_is_generating.set(false);
                        set_is_ai_searching.set(false);
                        set_ai_search_status.set(String::new());
                    },
                    _ => {}
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        Self {
            app_status, set_app_status, input_text, set_input_text, is_generating, set_is_generating,
            search_query, set_search_query, is_ai_searching, set_is_ai_searching, ai_search_results,
            set_ai_search_results, ai_search_status, set_ai_search_status, target_memo_id, set_target_memo_id,
            show_category_manager, set_show_category_manager, new_category_name, set_new_category_name,
            memo_list, set_memo_list, category_list, set_category_list, worker_status_msg, worker_progress,
            worker_store, memo_input_ref, search_input_ref,
        }
    }

    pub fn load_model(&self) {
        self.set_app_status.set(AppStatus::Loading);
        let input = WorkerInput { msg_type: "LOAD".to_string(), text: None, categories: None };
        self.worker_store.with_value(|w: &Worker| { w.post_message(&js_sys::JSON::parse(&serde_json::to_string(&input).unwrap()).unwrap()).unwrap(); });
    }

    pub fn analyze_memo(&self) {
        let prompt = self.input_text.get();
        if prompt.trim().is_empty() { return; }
        self.set_is_generating.set(true);
        self.set_target_memo_id.set(None); 
        
        let input = WorkerInput { msg_type: "PROMPT_CLASSIFY".to_string(), text: Some(prompt), categories: Some(self.category_list.get()) };
        self.worker_store.with_value(|w: &Worker| { w.post_message(&js_sys::JSON::parse(&serde_json::to_string(&input).unwrap()).unwrap()).unwrap(); });
    }

    pub fn trigger_vector_search(&self) {
        let query = self.search_query.get();
        if query.trim().is_empty() { self.set_ai_search_results.set(None); return; }
        self.set_is_ai_searching.set(true);
        self.set_ai_search_status.set("벡터 변환 준비 중...".to_string());
        
        let memos = self.memo_list.get();
        let mut memos_json_array = Vec::new();
        for m in memos { memos_json_array.push(serde_json::json!({ "id": m.id, "text": m.data.content.clone() })); }
        
        let payload = serde_json::json!({ "query": query, "memos": memos_json_array });
        let input = WorkerInput { msg_type: "VECTOR_SEARCH".to_string(), text: Some(payload.to_string()), categories: None };
        self.worker_store.with_value(|w: &Worker| { w.post_message(&js_sys::JSON::parse(&serde_json::to_string(&input).unwrap()).unwrap()).unwrap(); });
    }

    pub fn clear_search(&self) {
        self.set_search_query.set(String::new());
        self.set_ai_search_results.set(None);
        self.set_ai_search_status.set(String::new());
        if let Some(input) = self.search_input_ref.get() { input.set_value(""); }
    }

    pub fn export_data(&self) {
        let list = self.memo_list.get(); let cats = self.category_list.get();
        let export_obj = serde_json::json!({ "memos": list, "categories": cats });
        let json_str = export_obj.to_string();
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                let array = js_sys::Array::new(); array.push(&JsValue::from_str(&json_str));
                let options = web_sys::BlobPropertyBag::new(); options.set_type("application/json"); 
                if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&array, &options) {
                    if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                        if let Ok(a) = document.create_element("a") {
                            let a = a.dyn_into::<web_sys::HtmlAnchorElement>().unwrap(); a.set_href(&url);
                            let now = js_sys::Date::new_0();
                            let filename = format!("ai_memos_backup_{:04}{:02}{:02}.json", now.get_full_year(), now.get_month() + 1, now.get_date());
                            a.set_download(&filename); a.click(); let _ = web_sys::Url::revoke_object_url(&url);
                        }
                    }
                }
            }
        }
    }

    pub fn clear_memos(&self) {
        if let Some(window) = web_sys::window() {
            if window.confirm_with_message("정말 모든 메모를 영구적으로 삭제하시겠습니까?").unwrap_or(false) { self.set_memo_list.set(Vec::new()); }
        }
    }

    pub fn delete_memo(&self, target_id: usize) {
        if let Some(window) = web_sys::window() {
            if window.confirm_with_message("이 메모를 정말 삭제하시겠습니까?").unwrap_or(false) {
                self.set_memo_list.update(|list| { list.retain(|m| m.id != target_id); });
            }
        }
    }

    pub fn grouped_memos(&self) -> Vec<(String, Vec<MemoCard>)> {
        let mut list = self.memo_list.get();
        let query = self.search_query.get().to_lowercase();
        let ai_ids = self.ai_search_results.get();
        
        if let Some(ids) = ai_ids {
            let mut sorted_list = Vec::new();
            for id in ids { if let Some(m) = list.iter().find(|x| x.id == id) { sorted_list.push(m.clone()); } }
            list = sorted_list;
        } else if !query.trim().is_empty() {
            list.retain(|m| m.data.content.to_lowercase().contains(&query));
            list.sort_by(|a, b| b.timestamp.partial_cmp(&a.timestamp).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            list.sort_by(|a, b| b.timestamp.partial_cmp(&a.timestamp).unwrap_or(std::cmp::Ordering::Equal));
        }
        
        let current_categories = self.category_list.get();
        let mut groups = Vec::new();
        for cat in &current_categories {
            let items: Vec<_> = list.iter().filter(|m| m.data.category == *cat).cloned().collect();
            if !items.is_empty() && cat != "기타" && cat != "미분류" { groups.push((cat.clone(), items)); }
        }
        
        let other_items: Vec<_> = list.iter().filter(|m| m.data.category == "기타" || (!current_categories.contains(&m.data.category) && m.data.category != "미분류")).cloned().collect();
        if !other_items.is_empty() { groups.push(("기타".to_string(), other_items)); }
        
        let unclassified: Vec<_> = list.iter().filter(|m| m.data.category == "미분류").cloned().collect();
        if !unclassified.is_empty() { groups.push(("미분류".to_string(), unclassified)); }
        
        groups
    }
}