use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Worker};

// 마크다운 파서 엔진 부품
use pulldown_cmark::{Parser, Options, html};

// 분리된 모듈들을 가져옵니다.
use crate::models::*;
use crate::storage::*;

// 검색어 하이라이트 전용 엔진 컴포넌트
#[component]
fn HighlightText(text: String, query: ReadSignal<String>) -> impl IntoView {
    let parts = move || {
        let t = text.clone();
        let q = query.get(); 
        
        if q.trim().is_empty() {
            return vec![(t, false)];
        }
        
        let lower_t = t.to_lowercase();
        let lower_q = q.to_lowercase();
        
        if lower_t.len() != t.len() {
            return vec![(t, false)];
        }

        let mut res = Vec::new();
        let mut start = 0;
        
        while let Some(idx) = lower_t[start..].find(&lower_q) {
            let actual_idx = start + idx;
            if actual_idx > start {
                res.push((t[start..actual_idx].to_string(), false));
            }
            res.push((t[actual_idx..actual_idx + q.len()].to_string(), true));
            start = actual_idx + q.len();
        }
        if start < t.len() {
            res.push((t[start..].to_string(), false));
        }
        res
    };

    view! {
        <span>
            {move || parts().into_iter().map(|(part_text, is_highlight)| {
                if is_highlight {
                    leptos::either::Either::Left(view! { 
                        <mark style="background: #ffe066; color: #111; border-radius: 3px; padding: 1px 3px; font-weight: 700; box-shadow: 0 1px 2px rgba(0,0,0,0.1);">
                            {part_text}
                        </mark> 
                    })
                } else {
                    leptos::either::Either::Right(view! { <span>{part_text}</span> })
                }
            }).collect::<Vec<_>>()}
        </span>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let (app_status, set_app_status) = signal(AppStatus::NotLoaded);
    let (input_text, set_input_text) = signal(String::new());
    
    let (search_query, set_search_query) = signal(String::new());
    let (target_memo_id, set_target_memo_id) = signal::<Option<usize>>(None);
    
    let (show_category_manager, set_show_category_manager) = signal(false);
    let (new_category_name, set_new_category_name) = signal(String::new());
    
    let (memo_list, set_memo_list) = signal(load_memos());
    let (category_list, set_category_list) = signal(load_categories());

    Effect::new(move |_| save_memos(&memo_list.get()));
    Effect::new(move |_| save_categories(&category_list.get()));
    
    let (is_generating, set_is_generating) = signal(false);
    let (worker_status_msg, set_worker_status_msg) = signal(String::new());
    let (worker_progress, set_worker_progress) = signal(0.0f64); 
    let (current_ai_msg, set_current_ai_msg) = signal(String::new()); 

    let worker = {
        let options = web_sys::WorkerOptions::new();
        options.set_type(web_sys::WorkerType::Module);
        Worker::new_with_options("ai_worker.js?v=3", &options)
            .expect("워커 생성 실패!")
    };

    let worker_store = StoredValue::new(worker.clone());

    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(js_obj) = e.data().dyn_into::<js_sys::Object>() {
            let msg_type = js_sys::Reflect::get(&js_obj, &JsValue::from_str("type"))
                .unwrap_or(JsValue::NULL)
                .as_string()
                .unwrap_or_default();
                
            let text = js_sys::Reflect::get(&js_obj, &JsValue::from_str("text"))
                .unwrap_or(JsValue::NULL)
                .as_string()
                .unwrap_or_default();

            let progress = js_sys::Reflect::get(&js_obj, &JsValue::from_str("progress"))
                .unwrap_or(JsValue::from_f64(0.0))
                .as_f64()
                .unwrap_or(0.0);

            match msg_type.as_str() {
                "STATUS" => {
                    set_worker_status_msg.set(text);
                    set_worker_progress.set(progress);
                },
                "READY" => set_app_status.set(AppStatus::Ready),
                "TOKEN" => {
                    set_current_ai_msg.update(|msg| msg.push_str(&text));
                },
                "DONE" => {
                    let raw_json = current_ai_msg.get();
                    
                    let clean_json = if let (Some(start), Some(end)) = (raw_json.find('{'), raw_json.rfind('}')) {
                        if start <= end {
                            raw_json[start..=end].to_string()
                        } else {
                            raw_json.clone()
                        }
                    } else {
                        raw_json.clone()
                    };
                    
                    let now = js_sys::Date::new_0();
                    let timestamp = now.get_time();
                    let date_str = format!("{:04}-{:02}-{:02} {:02}:{:02}", 
                        now.get_full_year(), now.get_month() + 1, now.get_date(),
                        now.get_hours(), now.get_minutes());

                    match serde_json::from_str::<MemoData>(&clean_json) {
                        Ok(parsed_data) => {
                            set_memo_list.update(|list| {
                                if let Some(target_id) = target_memo_id.get_untracked() {
                                    if let Some(memo) = list.iter_mut().find(|m| m.id == target_id) {
                                        memo.data = parsed_data;
                                        // AI 분류 완료 시 최신 시간으로 업데이트해서 끌어올리기
                                        memo.timestamp = timestamp;
                                        memo.date_str = date_str.clone();
                                    }
                                } else {
                                    let new_id = list.iter().map(|m| m.id).max().unwrap_or(0) + 1;
                                    list.push(MemoCard { id: new_id, data: parsed_data, timestamp, date_str });
                                }
                            });
                        },
                        Err(_) => {
                            if let Some(_) = target_memo_id.get_untracked() {
                                if let Some(window) = web_sys::window() {
                                    let _ = window.alert_with_message("AI 재분류에 실패했습니다. (파싱 오류)");
                                }
                            } else {
                                set_memo_list.update(|list| {
                                    let new_id = list.iter().map(|m| m.id).max().unwrap_or(0) + 1;
                                    list.push(MemoCard { 
                                        id: new_id, 
                                        data: MemoData {
                                            category: "미분류".to_string(),
                                            title: "자동 분류 실패".to_string(),
                                            content: clean_json
                                        },
                                        timestamp,
                                        date_str
                                    });
                                });
                            }
                        }
                    }
                    
                    set_is_generating.set(false);
                    if target_memo_id.get_untracked().is_none() {
                        set_input_text.set(String::new());
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Some(el) = doc.get_element_by_id("main-memo-input") {
                                    if let Ok(ta) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
                                        ta.set_value("");
                                        let style = web_sys::HtmlElement::style(&ta);
                                        let _ = style.set_property("height", "auto");
                                    }
                                }
                            }
                        }
                    }
                    set_target_memo_id.set(None); 
                }
                "ERROR" => {
                    web_sys::console::error_1(&JsValue::from_str(&format!("🔥 에러: {}", text)));
                    set_app_status.set(AppStatus::Error(text));
                    set_is_generating.set(false);
                    set_target_memo_id.set(None);
                },
                _ => {}
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget(); 

    let load_model = move |_| {
        set_app_status.set(AppStatus::Loading);
        let input = WorkerInput { msg_type: "LOAD".to_string(), text: None, categories: None };
        worker_store.with_value(|w: &Worker| {
            let json = serde_json::to_string(&input).unwrap();
            w.post_message(&js_sys::JSON::parse(&json).unwrap()).unwrap();
        });
    };

    let analyze_memo = move |_| {
        let prompt = input_text.get();
        if prompt.trim().is_empty() { return; }
        
        set_is_generating.set(true);
        set_target_memo_id.set(None); 
        set_current_ai_msg.set(String::new()); 
        
        let input = WorkerInput { 
            msg_type: "PROMPT".to_string(), 
            text: Some(prompt),
            categories: Some(category_list.get()),
        };
        worker_store.with_value(|w: &Worker| {
            let json = serde_json::to_string(&input).unwrap();
            w.post_message(&js_sys::JSON::parse(&json).unwrap()).unwrap();
        });
    };

    let clear_memos = move |_| {
        if let Some(window) = web_sys::window() {
            if window.confirm_with_message("정말 모든 메모를 영구적으로 삭제하시겠습니까?").unwrap_or(false) {
                set_memo_list.set(Vec::new()); 
            }
        }
    };

    let export_data = move |_| {
        let list = memo_list.get();
        let cats = category_list.get();
        let export_obj = serde_json::json!({
            "memos": list,
            "categories": cats
        });
        
        let json_str = export_obj.to_string();
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                let array = js_sys::Array::new();
                array.push(&JsValue::from_str(&json_str));
                
                let options = web_sys::BlobPropertyBag::new();
                options.set_type("application/json"); 
                
                if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&array, &options) {
                    if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                        if let Ok(a) = document.create_element("a") {
                            let a = a.dyn_into::<web_sys::HtmlAnchorElement>().unwrap();
                            a.set_href(&url);
                            
                            // 🔥 [시간 정확도 보완] 브라우저의 로컬 시간을 정확히 가져와 패딩(0) 처리합니다.
                            let now = js_sys::Date::new_0();
                            let year = now.get_full_year();
                            let month = now.get_month() + 1; // 0-11
                            let date = now.get_date();
                            let hours = now.get_hours();
                            let minutes = now.get_minutes();
                            let seconds = now.get_seconds();

                            let filename = format!(
                                "ai_smart_memos_backup_{:04}{:02}{:02}_{:02}{:02}{:02}.json", 
                                year, month, date, hours, minutes, seconds
                            );
                            
                            a.set_download(&filename);
                            a.click();
                            let _ = web_sys::Url::revoke_object_url(&url);
                        }
                    }
                }
            }
        }
    };

    let trigger_import = move |_| {
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                if let Some(el) = doc.get_element_by_id("backup-import-input") {
                    let input = el.dyn_into::<web_sys::HtmlInputElement>().unwrap();
                    input.click();
                }
            }
        }
    };

    let on_file_change = move |ev: web_sys::Event| {
        let target = ev.target().unwrap().dyn_into::<web_sys::HtmlInputElement>().unwrap();
        if let Some(files) = target.files() {
            if let Some(file) = files.get(0) {
                let reader = web_sys::FileReader::new().unwrap();
                let reader_clone = reader.clone();
                
                let onload = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
                    if let Ok(result) = reader_clone.result() {
                        if let Some(text) = result.as_string() {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(memos) = parsed.get("memos") {
                                    if let Ok(m) = serde_json::from_value::<Vec<MemoCard>>(memos.clone()) {
                                        set_memo_list.set(m);
                                    }
                                }
                                if let Some(cats) = parsed.get("categories") {
                                    if let Ok(c) = serde_json::from_value::<Vec<String>>(cats.clone()) {
                                        set_category_list.set(c);
                                    }
                                }
                                if let Some(window) = web_sys::window() {
                                    let _ = window.alert_with_message("✅ 메모장이 완벽하게 복원되었습니다!");
                                }
                            } else {
                                if let Some(window) = web_sys::window() {
                                    let _ = window.alert_with_message("❌ 올바른 백업 파일이 아닙니다.");
                                }
                            }
                        }
                    }
                }) as Box<dyn FnMut(web_sys::Event)>);
                
                reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                onload.forget(); 
                let _ = reader.read_as_text(&file);
            }
        }
        target.set_value("");
    };

    let get_category_color = |cat: &str| -> String {
        match cat {
            "에러/버그" => "background: #ffebee; color: #c62828; border: 1px solid #ffcdd2",
            "코드 스니펫" => "background: #e3f2fd; color: #1565c0; border: 1px solid #bbdefb",
            "아이디어" => "background: #f3e5f5; color: #6a1b9a; border: 1px solid #e1bee7",
            "일상/회고" => "background: #e8f5e9; color: #2e7d32; border: 1px solid #c8e6c9",
            "미분류" => "background: #fff3e0; color: #ef6c00; border: 1px solid #ffe0b2",
            _ => "background: #f5f5f5; color: #424242; border: 1px solid #e0e0e0",
        }.to_string()
    };

    let grouped_memos = move || {
        let mut list = memo_list.get();
        let query = search_query.get().to_lowercase();
        
        if !query.trim().is_empty() {
            list.retain(|m| {
                m.data.title.to_lowercase().contains(&query) || 
                m.data.content.to_lowercase().contains(&query)
            });
        }
        
        list.sort_by(|a, b| b.timestamp.partial_cmp(&a.timestamp).unwrap_or(std::cmp::Ordering::Equal));
        
        let current_categories = category_list.get();
        let mut groups = Vec::new();
        
        for cat in &current_categories {
            let items: Vec<_> = list.iter().filter(|m| m.data.category == *cat).cloned().collect();
            if !items.is_empty() && cat != "기타" && cat != "미분류" {
                groups.push((cat.clone(), items));
            }
        }
        
        let other_items: Vec<_> = list.iter()
            .filter(|m| m.data.category == "기타" || (!current_categories.contains(&m.data.category) && m.data.category != "미분류"))
            .cloned()
            .collect();
            
        if !other_items.is_empty() {
            groups.push(("기타".to_string(), other_items));
        }
        
        let unclassified: Vec<_> = list.iter().filter(|m| m.data.category == "미분류").cloned().collect();
        if !unclassified.is_empty() {
            groups.push(("미분류".to_string(), unclassified));
        }
        
        groups
    };

    view! {
        <style>
            "@keyframes fadeIn { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }
             .search-bar { background: #f9f9f9; transition: all 0.2s ease; }
             .search-bar:focus { background: #fff; box-shadow: 0 0 0 2px rgba(0, 122, 255, 0.2); }
             
             .btn-hover { transition: transform 0.1s ease, box-shadow 0.1s ease; }
             .btn-hover:hover { transform: translateY(-1px); box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
             .btn-hover:active { transform: translateY(1px); box-shadow: none; }
             
             button:disabled { opacity: 0.5; cursor: not-allowed !important; transform: none !important; box-shadow: none !important; filter: grayscale(50%); }
             
             .magic-wand { transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1); }
             .magic-wand:hover:not(:disabled) { transform: scale(1.15) rotate(10deg); }
             .magic-wand:active:not(:disabled) { transform: scale(0.95); }
             
             .memo-card { transition: box-shadow 0.2s ease, transform 0.2s ease; }
             .memo-card:hover { box-shadow: 0 6px 16px rgba(0,0,0,0.08); transform: translateY(-2px); }
             
             .memo-input { transition: border-color 0.2s ease, box-shadow 0.2s ease; }
             .memo-input:focus { border-color: #007aff; box-shadow: 0 0 0 2px rgba(0, 122, 255, 0.2); }
             
             .memo-input:disabled { background-color: #f5f5f5; color: #888; cursor: not-allowed; border-color: #ddd; box-shadow: none; }
             
             .markdown-body { font-size: 0.95rem; color: #333; line-height: 1.6; }
             .markdown-body p { margin-top: 0; margin-bottom: 12px; }
             .markdown-body pre { background: #282c34; color: #abb2bf; padding: 14px; border-radius: 8px; overflow-x: auto; font-family: 'Courier New', Courier, monospace; font-size: 0.9em; box-shadow: inset 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 12px; }
             .markdown-body code { background: #f0f0f0; color: #e01e5a; padding: 3px 6px; border-radius: 4px; font-family: 'Courier New', Courier, monospace; font-size: 0.85em; }
             .markdown-body pre code { background: none; color: inherit; padding: 0; }
             .markdown-body blockquote { border-left: 4px solid #007aff; padding-left: 14px; color: #666; margin: 0 0 12px 0; background: #f8f9fa; padding-top: 8px; padding-bottom: 8px; border-radius: 0 6px 6px 0; }
             .markdown-body ul, .markdown-body ol { margin-top: 0; margin-bottom: 12px; padding-left: 24px; }
             .markdown-body h1, .markdown-body h2, .markdown-body h3 { border-bottom: 1px solid #eaeaea; padding-bottom: 6px; margin-top: 24px; margin-bottom: 12px; font-weight: 700; }"
        </style>
        
        <main style="max-width: 900px; margin: 0 auto; height: 100vh; display: flex; flex-direction: column; background: #fafafa; font-family: -apple-system, sans-serif;">
            <header style="padding: 16px 24px; background: #fff; border-bottom: 1px solid #eee; display: flex; flex-direction: column; gap: 16px; z-index: 10;">
                <div style="display: flex; justify-content: space-between; align-items: center;">
                    <div style="display: flex; align-items: center; gap: 12px;">
                        <h1 style="margin: 0; font-size: 1.2rem; color: #111; font-weight: 700;">"🧠 AI 스마트 메모 적재함"</h1>
                        
                        <Show when=move || app_status.get() == AppStatus::Ready>
                            <div style="display: flex; gap: 6px; margin-left: 8px; animation: fadeIn 0.3s;">
                                <button 
                                    class="btn-hover"
                                    on:click=export_data 
                                    style="padding: 4px 8px; font-size: 0.8rem; background: #f5f5f5; color: #333; border: 1px solid #ddd; border-radius: 4px; cursor: pointer; font-weight: bold;"
                                >"💾 백업"</button>
                                
                                <button 
                                    class="btn-hover"
                                    on:click=trigger_import 
                                    style="padding: 4px 8px; font-size: 0.8rem; background: #f5f5f5; color: #333; border: 1px solid #ddd; border-radius: 4px; cursor: pointer; font-weight: bold;"
                                >"📂 복원"</button>
                                
                                <input type="file" id="backup-import-input" accept=".json" style="display: none;" on:change=on_file_change />
                            </div>
                        </Show>
                    </div>

                    <div style="font-size: 0.85rem; color: #666;">
                        {move || match app_status.get() {
                            AppStatus::NotLoaded => "대기 중".to_string(),
                            AppStatus::Loading => "⏳ 엔진 로딩 중...".to_string(),
                            AppStatus::Ready => "🟢 엔진 활성화".to_string(),
                            AppStatus::Error(err) => format!("🔴 에러: {}", err),
                        }}
                    </div>
                </div>
                
                <Show when=move || app_status.get() == AppStatus::Ready && !memo_list.get().is_empty()>
                    <div style="display: flex; flex-direction: column; gap: 12px; animation: fadeIn 0.3s;">
                        <div style="display: flex; gap: 8px;">
                            <button 
                                class="btn-hover"
                                on:click=move |_| set_show_category_manager.update(|s| *s = !*s)
                                style="padding: 4px 8px; font-size: 0.8rem; background: #e3f2fd; color: #1565c0; border: 1px solid #bbdefb; border-radius: 4px; cursor: pointer; font-weight: bold;"
                            >
                                "⚙️ 분류 관리"
                            </button>
                            
                            <button 
                                class="btn-hover"
                                on:click=clear_memos 
                                style="padding: 4px 8px; font-size: 0.8rem; background: #ffebee; color: #c62828; border: 1px solid #ffcdd2; border-radius: 4px; cursor: pointer; font-weight: bold;"
                            >
                                "전체 삭제 🗑️"
                            </button>
                        </div>
                        
                        <div style="position: relative; width: 100%;">
                            <span style="position: absolute; left: 12px; top: 10px; font-size: 1rem;">"🔍"</span>
                            <input 
                                id="main-search-input"
                                type="text"
                                class="search-bar"
                                placeholder="검색어를 입력하세요 (제목, 내용 실시간 검색)..."
                                on:input=move |_| {
                                    if let Some(window) = web_sys::window() {
                                        if let Some(doc) = window.document() {
                                            if let Some(el) = doc.get_element_by_id("main-search-input") {
                                                if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                                                    set_search_query.set(input.value());
                                                }
                                            }
                                        }
                                    }
                                }
                                on:keyup=move |_| {
                                    if let Some(window) = web_sys::window() {
                                        if let Some(doc) = window.document() {
                                            if let Some(el) = doc.get_element_by_id("main-search-input") {
                                                if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                                                    set_search_query.set(input.value());
                                                }
                                            }
                                        }
                                    }
                                }
                                style="width: 100%; padding: 10px 10px 10px 38px; border: 1px solid #ddd; border-radius: 8px; font-size: 0.95rem; outline: none; box-sizing: border-box;"
                            />
                            <Show when=move || !search_query.get().is_empty()>
                                <button 
                                    on:click=move |_| {
                                        set_search_query.set(String::new());
                                        if let Some(window) = web_sys::window() {
                                            if let Some(doc) = window.document() {
                                                if let Some(el) = doc.get_element_by_id("main-search-input") {
                                                    if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                                                        input.set_value("");
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    style="position: absolute; right: 12px; top: 8px; background: none; border: none; cursor: pointer; color: #999; font-size: 1rem;"
                                >
                                    "✕"
                                </button>
                            </Show>
                        </div>
                    </div>
                </Show>
            </header>

            <div style="flex: 1; overflow-y: auto; padding: 24px; display: flex; flex-direction: column; gap: 32px;">
                
                <Show when=move || app_status.get() == AppStatus::Ready && show_category_manager.get() && !memo_list.get().is_empty()>
                    <div style="background: #fff; border-radius: 12px; padding: 16px; box-shadow: 0 2px 10px rgba(0,0,0,0.05); border: 1px solid #eaeaea; animation: fadeIn 0.3s;">
                        <h3 style="margin-top: 0; margin-bottom: 12px; font-size: 1rem; color: #333;">"현재 활성화된 카테고리"</h3>
                        <div style="display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 16px;">
                            <For
                                each=move || category_list.get().into_iter().filter(|c| c != "기타" && c != "미분류")
                                key=|c| c.clone()
                                children=move |cat| {
                                    let cat_display = cat.clone();
                                    let cat_for_edit = cat.clone();
                                    let cat_for_delete = cat.clone();

                                    let on_edit = move |_| {
                                        let current_cat = cat_for_edit.clone();
                                        if let Some(window) = web_sys::window() {
                                            if let Ok(Some(new_name)) = window.prompt_with_message_and_default("✏️ 새로운 카테고리 이름을 입력하세요:", &current_cat) {
                                                let new_name = new_name.trim().to_string();
                                                if !new_name.is_empty() && new_name != current_cat && !category_list.get().contains(&new_name) {
                                                    let memos = memo_list.get();
                                                    let count = memos.iter().filter(|m| m.data.category == current_cat).count();
                                                    
                                                    let mut should_update_memos = true;
                                                    if count > 0 {
                                                        should_update_memos = window.confirm_with_message(&format!("'{}' 카테고리를 '{}'(으)로 변경합니다.\n\n기존에 작성된 메모 {}개도 모두 새 이름으로 일괄 업데이트(덮어쓰기) 할까요?", current_cat, new_name, count)).unwrap_or(false);
                                                    }

                                                    set_category_list.update(|list| {
                                                        if let Some(pos) = list.iter().position(|x| *x == current_cat) {
                                                            list[pos] = new_name.clone();
                                                        }
                                                    });

                                                    if should_update_memos {
                                                        set_memo_list.update(|list| {
                                                            for m in list.iter_mut() {
                                                                if m.data.category == current_cat {
                                                                    m.data.category = new_name.clone();
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    };

                                    let on_delete = move |_| {
                                        let current_cat = cat_for_delete.clone();
                                        if let Some(window) = web_sys::window() {
                                            let memos = memo_list.get();
                                            let count = memos.iter().filter(|m| m.data.category == current_cat).count();

                                            if count == 0 {
                                                set_category_list.update(|list| list.retain(|c| c != &current_cat));
                                            } else {
                                                let delete_memos = window.confirm_with_message(&format!("🗑️ '{}' 카테고리를 삭제합니다.\n\n이 바구니에 담긴 {}개의 메모도 함께 [영구 삭제] 할까요?\n\n(※ '취소'를 누르시면 다른 카테고리로 안전하게 이동시킬 수 있습니다.)", current_cat, count)).unwrap_or(false);
                                                
                                                if delete_memos {
                                                    set_category_list.update(|list| list.retain(|c| c != &current_cat));
                                                    set_memo_list.update(|list| list.retain(|m| m.data.category != current_cat));
                                                } else {
                                                    if let Ok(Some(target_cat)) = window.prompt_with_message_and_default(&format!("그렇다면 기존 메모 {}개를 어느 카테고리로 옮길까요?\n\n(이동할 대상 카테고리를 입력해주세요. 빈칸으로 두시고 확인을 누르시면 '기타'로 자동 이동됩니다.)", count), "기타") {
                                                        let target_cat = target_cat.trim().to_string();
                                                        let final_target = if target_cat.is_empty() { "기타".to_string() } else { target_cat.clone() };
                                                        
                                                        set_category_list.update(|list| {
                                                            list.retain(|c| c != &current_cat);
                                                            if final_target != "기타" && final_target != "미분류" && !list.contains(&final_target) {
                                                                list.insert(list.len().saturating_sub(2), final_target.clone());
                                                            }
                                                        });

                                                        set_memo_list.update(|list| {
                                                            for m in list.iter_mut() {
                                                                if m.data.category == current_cat {
                                                                    m.data.category = final_target.clone();
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    };

                                    view! {
                                        <span style="display: inline-flex; align-items: center; gap: 6px; padding: 6px 12px; background: #f0f0f0; border-radius: 20px; font-size: 0.85rem; color: #333; font-weight: 500;">
                                            {cat_display}
                                            <div style="display: flex; gap: 4px; margin-left: 4px; border-left: 1px solid #ccc; padding-left: 6px;">
                                                <button
                                                    on:click=on_edit
                                                    title="이름 변경"
                                                    style="background: none; border: none; cursor: pointer; color: #666; padding: 0; font-size: 0.9rem;"
                                                >"✏️"</button>
                                                <button
                                                    on:click=on_delete
                                                    title="카테고리 삭제"
                                                    style="background: none; border: none; cursor: pointer; color: #d32f2f; padding: 0; font-size: 0.9rem;"
                                                >"✕"</button>
                                            </div>
                                        </span>
                                    }
                                }
                            />
                        </div>
                        <div style="display: flex; gap: 8px;">
                            <input
                                id="category-add-input"
                                type="text"
                                placeholder="새 카테고리 이름 입력 (예: 유튜브, 회의록)"
                                on:input=move |ev| set_new_category_name.set(event_target_value(&ev))
                                style="flex: 1; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; outline: none; font-size: 0.95rem;"
                            />
                            <button
                                class="btn-hover"
                                on:click=move |_| {
                                    let new_cat = new_category_name.get().trim().to_string();
                                    if !new_cat.is_empty() && !category_list.get().contains(&new_cat) {
                                        set_category_list.update(|list| {
                                            list.insert(list.len().saturating_sub(2), new_cat);
                                        });
                                        set_new_category_name.set(String::new());
                                        if let Some(window) = web_sys::window() {
                                            if let Some(doc) = window.document() {
                                                if let Some(el) = doc.get_element_by_id("category-add-input") {
                                                    if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                                                        input.set_value("");
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                style="padding: 10px 20px; background: #222; color: #fff; border: none; border-radius: 6px; cursor: pointer; font-weight: bold;"
                            >"추가"</button>
                        </div>
                    </div>
                </Show>

                // 🔥 [보안 고지 업데이트] 공용 PC 사용 위험성에 대한 치명적인 경고를 눈에 띄게 추가했습니다!
                <Show when=move || app_status.get() == AppStatus::NotLoaded>
                    <div style="background: #fff; border-radius: 16px; padding: 40px; box-shadow: 0 10px 30px rgba(0,0,0,0.08); border: 1px solid #eaeaea; max-width: 700px; margin: 40px auto; animation: fadeIn 0.6s cubic-bezier(0.2, 0.8, 0.2, 1);">
                        
                        <div style="text-align: center; margin-bottom: 32px;">
                            <span style="font-size: 3rem; display: block; margin-bottom: 16px;">"🧠"</span>
                            <h2 style="margin: 0; color: #111; font-size: 1.8rem; font-weight: 800;">"AI 스마트 메모 적재함 시작하기"</h2>
                            <p style="color: #666; font-size: 1rem; margin-top: 12px;">"오직 당신의 브라우저에서만 돌아가는, 세상에서 가장 안전하고 빠른 AI 메모장입니다."</p>
                        </div>

                        <div style="margin-bottom: 32px;">
                            <h3 style="font-size: 1.1rem; color: #007aff; margin-bottom: 16px; border-bottom: 2px solid #e3f2fd; padding-bottom: 8px; display: inline-block;">"✨ 적재함 핵심 기능"</h3>
                            <div style="display: grid; grid-template-columns: 1fr; gap: 12px; font-size: 0.95rem; color: #444;">
                                <div style="display: flex; align-items: flex-start; gap: 10px;">
                                    <span style="font-size: 1.2rem;">"🔒"</span>
                                    <span><b>"100% 온디바이스 AI:"</b>" 입력하신 모든 메모는 외부 서버로 전송되지 않습니다. 오프라인에서도 AI가 내용을 분석해 카테고리를 완벽하게 자동 분류합니다."</span>
                                </div>
                                <div style="display: flex; align-items: flex-start; gap: 10px;">
                                    <span style="font-size: 1.2rem;">"⚡"</span>
                                    <span><b>"초고속 실시간 검색:"</b>" 한글 조합 지연(IME)을 완벽히 해결하여, 키보드를 누르는 즉시 번개처럼 하이라이트 되는 쾌적한 검색을 제공합니다."</span>
                                </div>
                                <div style="display: flex; align-items: flex-start; gap: 10px;">
                                    <span style="font-size: 1.2rem;">"📝"</span>
                                    <span><b>"스마트 마크다운 렌더링:"</b>" 코드 블록, 인용구, 볼드체 등 노션(Notion) 스타일의 깔끔한 뷰 모드와 직관적인 인라인 수정 기능을 제공합니다."</span>
                                </div>
                            </div>
                        </div>

                        <div style="background: #fff3e0; border-left: 4px solid #d84315; border-radius: 4px 8px 8px 4px; padding: 16px 20px; margin-bottom: 20px; color: #bf360c;">
                            <h4 style="margin: 0 0 8px 0; font-size: 1.05rem; display: flex; align-items: center; gap: 6px;">"🚨 보안 및 데이터 저장 주의사항 (필독)"</h4>
                            <p style="margin: 0; font-size: 0.9rem; line-height: 1.6;">
                                "본 서비스는 로그인(인증) 절차 없이 작동하며, 모든 메모는 현재 사용 중인 기기의 "
                                <b>"로컬 스토리지(Local Storage)"</b>
                                "에 무방비 상태로 저장됩니다."
                                <br/><br/>
                                <span style="color: #d32f2f; font-weight: bold; background: #ffcdd2; padding: 2px 4px; border-radius: 3px;">"절대 공용 PC(PC방, 도서관, 타인의 컴퓨터)에서 이 앱을 사용하지 마십시오!"</span>
                                " 다음 사용자가 이 주소로 접속하면 귀하의 모든 메모를 열람할 수 있습니다. 오직 개인이 독점 소유한 PC/모바일에서만 사용을 권장합니다."
                                <br/><br/>
                                "또한, "<b>"브라우저 방문 기록(캐시)을 삭제하면 저장된 모든 메모가 영구적으로 삭제됩니다."</b>
                                " 소중한 데이터는 반드시 화면 우측 상단의 "
                                <b>"[💾 백업]"</b>
                                " 버튼을 눌러 주기적으로 안전하게 보관하십시오."
                            </p>
                        </div>

                        <div style="background: #f8f9fa; border: 1px solid #e0e0e0; border-radius: 8px; padding: 16px 20px; margin-bottom: 32px; color: #555;">
                            <h4 style="margin: 0 0 8px 0; font-size: 1rem; color: #333;">"🖥️ 시스템 요구사항 및 모델 고지"</h4>
                            <ul style="padding-left: 20px; margin: 0; font-size: 0.85rem; line-height: 1.6;">
                                <li style="margin-bottom: 4px;">
                                    <b>"AI 모델 명시:"</b> 
                                    " 본 애플리케이션은 브라우저 내장형 WebLLM 기술을 사용하여 "
                                    <code>"Llama-3.1-8B-Instruct-q4f32_1-MLC"</code>
                                    " 모델을 구동합니다. AI의 분류 결과는 간혹 부정확하거나 사용자의 의도와 다를 수 있습니다."
                                </li>
                                <li>
                                    <b>"최소 하드웨어 요구사항:"</b> 
                                    " 본 앱은 4-bit 양자화(Quantization) 기술을 적용하여 80억 개(8B)의 파라미터를 가진 대형 모델을 브라우저에 압축 탑재했습니다. 원활한 구동을 위해 "
                                    <b>"WebGPU"</b>
                                    " 환경과 "
                                    <b>"최소 6GB 이상의 VRAM (Mac 또는 내장 GPU의 경우 시스템 RAM 16GB 이상)"</b>
                                    "이 강력히 권장됩니다. 사양이 부족한 기기에서는 모델 로딩 시 브라우저 탭이 강제 종료될 수 있습니다."
                                </li>
                            </ul>
                        </div>

                        <div style="text-align: center;">
                            <button class="btn-hover" style="padding: 16px 40px; font-size: 1.15rem; background: #000; color: #fff; border: none; border-radius: 8px; cursor: pointer; font-weight: 800; box-shadow: 0 4px 15px rgba(0,0,0,0.2); transition: all 0.2s ease;" on:click=load_model>
                                "위 내용을 확인했으며, AI 엔진을 적재합니다 🚀"
                            </button>
                            <div style="margin-top: 12px; font-size: 0.8rem; color: #999;">
                                "첫 로딩 시 AI 모델(약 4.5GB ~ 5GB) 다운로드를 위해 네트워크 환경에 따라 수 분이 소요될 수 있습니다."
                            </div>
                        </div>
                    </div>
                </Show>

                <Show when=move || app_status.get() == AppStatus::Loading>
                    <div style="margin: auto; width: 100%; max-width: 400px; text-align: center; animation: fadeIn 0.5s;">
                        <h3 style="color: #333; margin-bottom: 16px;">"로컬 VRAM에 AI 모델을 다운로드 및 적재 중입니다..."</h3>
                        <div style="width: 100%; background: #e1e4e8; border-radius: 8px; height: 10px; overflow: hidden; margin-bottom: 12px;">
                            <div style=move || format!("width: {:.1}%; height: 100%; background: #007aff; transition: width 0.3s ease;", worker_progress.get() * 100.0)></div>
                        </div>
                        <div style="font-size: 0.85rem; color: #666; word-break: break-all;">
                            {move || worker_status_msg.get()}
                        </div>
                    </div>
                </Show>

                <Show when=move || app_status.get() == AppStatus::Ready>
                    <div style="background: #fff; border-radius: 12px; padding: 16px; box-shadow: 0 2px 10px rgba(0,0,0,0.05); border: 1px solid #eaeaea;">
                        <textarea 
                            id="main-memo-input"
                            class="memo-input"
                            style="width: 100%; min-height: 100px; padding: 12px; border: 1px solid #ddd; border-radius: 8px; resize: none; overflow: hidden; font-size: 1rem; outline: none; font-family: inherit; box-sizing: border-box;"
                            placeholder="아무 생각이나 코드, 일상, 에러 로그를 대충 던져주세요. AI가 알아서 분류해 드립니다."
                            on:input=move |ev| {
                                let target = ev.target().unwrap().dyn_into::<web_sys::HtmlTextAreaElement>().unwrap();
                                let style = web_sys::HtmlElement::style(&target); 
                                let _ = style.set_property("height", "auto");
                                let scroll_height = target.scroll_height();
                                let _ = style.set_property("height", &format!("{}px", scroll_height));
                                set_input_text.set(target.value());
                            }
                            disabled=move || is_generating.get()
                        ></textarea>
                        <div style="display: flex; justify-content: flex-end; margin-top: 12px;">
                            <button 
                                class="btn-hover"
                                style="padding: 10px 24px; background: #007aff; color: #fff; border: none; border-radius: 6px; font-weight: 600; cursor: pointer;"
                                disabled=move || is_generating.get() || input_text.get().trim().is_empty()
                                on:click=analyze_memo
                            > 
                                {move || if is_generating.get() { "분석 및 적재 중... ⏳".to_string() } else { "분류해서 적재하기 📥".to_string() }}
                            </button>
                        </div>
                    </div>

                    <div style="display: flex; flex-direction: column; gap: 32px;">
                        <For
                            each=grouped_memos
                            key=|(cat, items)| {
                                let t_sum: f64 = items.iter().map(|m| m.timestamp).sum();
                                format!("{}-{}-{}", cat, items.len(), t_sum)
                            }
                            children=move |(category, items)| {
                                view! {
                                    <div style="display: flex; flex-direction: column; gap: 16px; animation: fadeIn 0.4s ease-out;">
                                        <div style="display: flex; align-items: center; gap: 8px; border-bottom: 2px solid #eee; padding-bottom: 8px;">
                                            <h2 style="margin: 0; font-size: 1.3rem; color: #222;">{category.clone()}</h2>
                                            <span style="background: #f0f0f0; color: #666; font-size: 0.8rem; padding: 2px 8px; border-radius: 12px; font-weight: 700;">
                                                {items.len()}
                                            </span>
                                        </div>
                                        
                                        <div style="display: grid; grid-template-columns: 1fr; gap: 16px;">
                                            <For
                                                each=move || items.clone()
                                                key=|memo| format!("{}-{}", memo.id, memo.timestamp)
                                                children=move |memo| {
                                                    let memo_id = memo.id;
                                                    
                                                    // 궁극의 소유권 해결사: 개인 금고(StoredValue) 발급!
                                                    let memo_store = StoredValue::new(memo.clone());
                                                    
                                                    // 편집 상태도 글로벌 변수가 아니라 이 카드만의 독립적인 상태로 분리했습니다.
                                                    let (is_editing, set_is_editing) = signal(false);
                                                    let (edit_title, set_edit_title) = signal(memo.data.title.clone());
                                                    let (edit_content, set_edit_content) = signal(memo.data.content.clone());
                                                    
                                                    view! {
                                                        <div class="memo-card" style="background: #fff; border-radius: 12px; padding: 20px; box-shadow: 0 1px 4px rgba(0,0,0,0.08); border: 1px solid #eaeaea; position: relative;">
                                                            <Show 
                                                                when=move || is_editing.get()
                                                                fallback=move || {
                                                                    // 보기 모드 (View Mode)
                                                                    view! {
                                                                        <div>
                                                                            <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 16px; gap: 16px;">
                                                                                <h3 style="margin: 0; font-size: 1.1rem; color: #111; flex: 1; line-height: 1.4; word-break: break-all;">
                                                                                    <HighlightText text=memo_store.with_value(|m| m.data.title.clone()) query=search_query />
                                                                                </h3>
                                                                                
                                                                                <span style="font-size: 0.8rem; color: #888; display: flex; flex-direction: column; align-items: flex-end; gap: 6px; flex-shrink: 0;">
                                                                                    <span>{memo_store.with_value(|m| m.date_str.clone())}</span>
                                                                                    
                                                                                    <div style="display: flex; gap: 4px; align-items: center;">
                                                                                        // 수정 버튼
                                                                                        <button
                                                                                            class="magic-wand"
                                                                                            title="본문 직접 수정하기"
                                                                                            on:click=move |_| {
                                                                                                memo_store.with_value(|m| {
                                                                                                    set_edit_title.set(m.data.title.clone());
                                                                                                    set_edit_content.set(m.data.content.clone());
                                                                                                    set_is_editing.set(true);
                                                                                                });
                                                                                            }
                                                                                            style="background: none; border: 1px solid #ddd; border-radius: 4px; cursor: pointer; padding: 2px 6px; font-size: 0.8rem; background-color: #f9f9f9;"
                                                                                        >
                                                                                            "✏️"
                                                                                        </button>
                                                                                        
                                                                                        // AI 재분류 버튼
                                                                                        <button
                                                                                            class="magic-wand"
                                                                                            title="이 메모만 AI에게 다시 분류시키기"
                                                                                            on:click=move |_| {
                                                                                                if is_generating.get() { return; }
                                                                                                memo_store.with_value(|m| {
                                                                                                    set_target_memo_id.set(Some(m.id));
                                                                                                    set_is_generating.set(true);
                                                                                                    set_current_ai_msg.set(String::new());
                                                                                                    
                                                                                                    let input = WorkerInput {
                                                                                                        msg_type: "PROMPT".to_string(),
                                                                                                        text: Some(m.data.content.clone()),
                                                                                                        categories: Some(category_list.get()),
                                                                                                    };
                                                                                                    worker_store.with_value(|w: &Worker| {
                                                                                                        let json = serde_json::to_string(&input).unwrap();
                                                                                                        w.post_message(&js_sys::JSON::parse(&json).unwrap()).unwrap();
                                                                                                    });
                                                                                                });
                                                                                            }
                                                                                            disabled=move || is_generating.get() || app_status.get() != AppStatus::Ready
                                                                                            style="background: none; border: 1px solid #ddd; border-radius: 4px; cursor: pointer; padding: 2px 6px; font-size: 0.8rem; background-color: #f9f9f9;"
                                                                                        >
                                                                                            {move || if target_memo_id.get() == Some(memo_id) { "⏳" } else { "🪄" }}
                                                                                        </button>
                                                                                        
                                                                                        // 수동 카테고리 이동 셀렉트
                                                                                        <select
                                                                                            title="수동으로 카테고리 이동"
                                                                                            on:change=move |ev| {
                                                                                                let new_cat = event_target_value(&ev);
                                                                                                set_memo_list.update(|list| {
                                                                                                    if let Some(m) = list.iter_mut().find(|x| x.id == memo_id) {
                                                                                                        m.data.category = new_cat;
                                                                                                    }
                                                                                                });
                                                                                            }
                                                                                            style=move || format!("padding: 2px 6px; border-radius: 4px; font-size: 0.75rem; font-weight: 700; border: none; outline: none; cursor: pointer; {}", get_category_color(&memo_store.with_value(|m| m.data.category.clone())))
                                                                                        >
                                                                                            <For
                                                                                                each=move || {
                                                                                                    let mut cats = category_list.get();
                                                                                                    if !cats.contains(&"기타".to_string()) { cats.push("기타".to_string()); }
                                                                                                    if !cats.contains(&"미분류".to_string()) { cats.push("미분류".to_string()); }
                                                                                                    cats
                                                                                                }
                                                                                                key=|c| c.clone()
                                                                                                children=move |cat| {
                                                                                                    let is_selected = cat == memo_store.with_value(|m| m.data.category.clone());
                                                                                                    view! { <option value=cat.clone() selected=is_selected>{cat.clone()}</option> }
                                                                                                }
                                                                                            />
                                                                                        </select>
                                                                                    </div>
                                                                                </span>
                                                                            </div>
                                                                            
                                                                            // 마크다운 스마트 스위칭 모드!
                                                                            <Show 
                                                                                when=move || search_query.get().trim().is_empty() 
                                                                                fallback=move || {
                                                                                    view! {
                                                                                        <div style="font-size: 0.95rem; color: #444; line-height: 1.6; white-space: pre-wrap;">
                                                                                            <HighlightText text=memo_store.with_value(|m| m.data.content.clone()) query=search_query />
                                                                                        </div>
                                                                                    }
                                                                                }
                                                                            >
                                                                                {
                                                                                    move || {
                                                                                        let mut options = Options::empty();
                                                                                        options.insert(Options::ENABLE_TABLES);
                                                                                        options.insert(Options::ENABLE_STRIKETHROUGH);
                                                                                        options.insert(Options::ENABLE_TASKLISTS);
                                                                                        
                                                                                        // 매번 렌더링될 때마다 금고에서 안전하게 꺼내옵니다.
                                                                                        let content = memo_store.with_value(|m| m.data.content.clone());
                                                                                        let parser = Parser::new_ext(&content, options);
                                                                                        
                                                                                        let mut parsed_md_html = String::new();
                                                                                        html::push_html(&mut parsed_md_html, parser);
                                                                                        
                                                                                        view! {
                                                                                            <div class="markdown-body" inner_html=parsed_md_html></div>
                                                                                        }
                                                                                    }
                                                                                }
                                                                            </Show>
                                                                        </div>
                                                                    }
                                                                }
                                                            >
                                                                // 수정 모드 (Edit Mode)
                                                                <div style="display: flex; flex-direction: column; gap: 12px; width: 100%; animation: fadeIn 0.2s;">
                                                                    <input 
                                                                        type="text" 
                                                                        prop:value=move || edit_title.get()
                                                                        on:input=move |ev| set_edit_title.set(event_target_value(&ev))
                                                                        style="padding: 10px; border: 1px solid #007aff; border-radius: 6px; font-size: 1.1rem; font-weight: bold; outline: none; box-shadow: 0 0 0 2px rgba(0,122,255,0.1);"
                                                                    />
                                                                    <textarea 
                                                                        prop:value=move || edit_content.get()
                                                                        on:input=move |ev| {
                                                                            let target = ev.target().unwrap().dyn_into::<web_sys::HtmlTextAreaElement>().unwrap();
                                                                            let style = web_sys::HtmlElement::style(&target); 
                                                                            let _ = style.set_property("height", "auto");
                                                                            let scroll_height = target.scroll_height();
                                                                            let _ = style.set_property("height", &format!("{}px", scroll_height));
                                                                            set_edit_content.set(target.value());
                                                                        }
                                                                        style="width: 100%; min-height: 150px; padding: 12px; border: 1px solid #007aff; border-radius: 6px; resize: none; overflow: hidden; font-size: 0.95rem; outline: none; font-family: inherit; box-sizing: border-box; line-height: 1.6; box-shadow: 0 0 0 2px rgba(0,122,255,0.1);"
                                                                    ></textarea>
                                                                    
                                                                    <div style="display: flex; justify-content: flex-end; gap: 8px;">
                                                                        <button 
                                                                            class="btn-hover"
                                                                            on:click=move |_| set_is_editing.set(false) 
                                                                            style="padding: 8px 16px; background: #f5f5f5; color: #555; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 0.9rem; font-weight: bold;"
                                                                        >
                                                                            "취소"
                                                                        </button>
                                                                        <button 
                                                                            class="btn-hover"
                                                                            on:click=move |_| { 
                                                                                set_memo_list.update(|list| {
                                                                                    if let Some(m) = list.iter_mut().find(|x| x.id == memo_id) {
                                                                                        m.data.title = edit_title.get();
                                                                                        m.data.content = edit_content.get();
                                                                                        
                                                                                        // 수정 완료 시 시간을 업데이트하여 최상단으로 끌어올립니다!
                                                                                        let now = js_sys::Date::new_0();
                                                                                        m.timestamp = now.get_time();
                                                                                        m.date_str = format!("{:04}-{:02}-{:02} {:02}:{:02}", 
                                                                                            now.get_full_year(), now.get_month() + 1, now.get_date(),
                                                                                            now.get_hours(), now.get_minutes());
                                                                                    }
                                                                                });
                                                                                set_is_editing.set(false);
                                                                            }
                                                                            style="padding: 8px 16px; background: #007aff; color: #fff; border: none; border-radius: 6px; cursor: pointer; font-size: 0.9rem; font-weight: bold;"
                                                                        >
                                                                            "💾 저장"
                                                                        </button>
                                                                    </div>
                                                                </div>
                                                            </Show>
                                                        </div>
                                                    }
                                                }
                                            />
                                        </div>
                                    </div>
                                }
                            }
                        />
                        
                        <Show when=move || app_status.get() == AppStatus::Ready && memo_list.get().is_empty()>
                            <div style="text-align: center; color: #999; padding: 40px 0; font-size: 0.9rem; animation: fadeIn 0.3s;">
                                "아직 적재된 메모가 없습니다. 첫 메모를 입력해 보세요!"
                            </div>
                        </Show>
                        
                        <Show when=move || app_status.get() == AppStatus::Ready && !memo_list.get().is_empty() && !search_query.get().trim().is_empty() && grouped_memos().is_empty()>
                            <div style="text-align: center; color: #999; padding: 40px 0; font-size: 0.9rem;">
                                "검색어와 일치하는 메모가 없습니다. 😅"
                            </div>
                        </Show>
                    </div>
                </Show>
            </div>
        </main>
    }
}