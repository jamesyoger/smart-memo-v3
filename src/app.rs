use leptos::prelude::*;
use leptos::html::Input;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use pulldown_cmark::{Parser, Options, html};
use crate::store::AppStore;
use crate::models::{AppStatus, MemoCard};

// 👉 Pro-Level: Cargo.toml 수정 없이 js_sys::Reflect를 활용한 동적 프로퍼티 접근 기법
fn is_mobile_device() -> bool {
    if let Some(window) = web_sys::window() {
        if let Ok(navigator) = js_sys::Reflect::get(&window, &JsValue::from_str("navigator")) {
            if let Ok(user_agent) = js_sys::Reflect::get(&navigator, &JsValue::from_str("userAgent")) {
                if let Some(ua) = user_agent.as_string() {
                    let ua_lower = ua.to_lowercase();
                    return ua_lower.contains("mobi") || ua_lower.contains("android") || ua_lower.contains("iphone") || ua_lower.contains("ipad");
                }
            }
        }
    }
    false
}

#[component]
fn HighlightText(text: String, query: ReadSignal<String>) -> impl IntoView {
    let parts = move || {
        let t = text.clone();
        let q = query.get();
        if q.trim().is_empty() { return vec![(t, false)]; }

        let lower_t = t.to_lowercase();
        let lower_q = q.to_lowercase();
        if lower_t.len() != t.len() { return vec![(t, false)]; }

        let mut res = Vec::new();
        let mut start = 0;
        
        while let Some(idx) = lower_t[start..].find(&lower_q) {
            let actual_idx = start + idx;
            if actual_idx > start { res.push((t[start..actual_idx].to_string(), false)); }
            res.push((t[actual_idx..actual_idx + q.len()].to_string(), true));
            start = actual_idx + q.len();
        }
        if start < t.len() { res.push((t[start..].to_string(), false)); }
        res
    };

    view! {
        <span>
            {move || parts().into_iter().map(|(part_text, is_highlight)| {
                if is_highlight {
                    leptos::either::Either::Left(view! { <mark style="background: #ffe066; color: #111; border-radius: 3px; padding: 1px 3px; font-weight: 700;">{part_text}</mark> })
                } else {
                    leptos::either::Either::Right(view! { <span>{part_text}</span> })
                }
            }).collect::<Vec<_>>()}
        </span>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let store = AppStore::new();

    let file_input_ref = NodeRef::<Input>::new();
    let category_add_input_ref = NodeRef::<Input>::new();

    let trigger_import = move |_| { if let Some(input) = file_input_ref.get() { input.click(); } };

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
                                    if let Ok(m) = serde_json::from_value::<Vec<MemoCard>>(memos.clone()) { store.set_memo_list.set(m); }
                                }
                                if let Some(cats) = parsed.get("categories") {
                                    if let Ok(c) = serde_json::from_value::<Vec<String>>(cats.clone()) { store.set_category_list.set(c); }
                                }
                                if let Some(window) = web_sys::window() { let _ = window.alert_with_message("✅ 복원 완료!"); }
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

    view! {
        <style>
            "@keyframes fadeIn { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }
             .search-bar { background: #f9f9f9; transition: all 0.2s ease; }
             .search-bar:focus { background: #fff; box-shadow: 0 0 0 2px rgba(0, 122, 255, 0.2); }
             .btn-hover { transition: transform 0.1s ease, box-shadow 0.1s ease; }
             .btn-hover:hover:not(:disabled) { transform: translateY(-1px); box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
             .btn-hover:active:not(:disabled) { transform: translateY(1px); box-shadow: none; }
             button:disabled { opacity: 0.5; cursor: not-allowed !important; transform: none !important; box-shadow: none !important; filter: grayscale(50%); }
             .magic-wand { transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1); }
             .magic-wand:hover:not(:disabled) { transform: scale(1.15) rotate(10deg); }
             .memo-card { transition: box-shadow 0.2s ease, transform 0.2s ease; }
             .memo-card:hover { box-shadow: 0 6px 16px rgba(0,0,0,0.08); transform: translateY(-2px); }
             .memo-input { transition: border-color 0.2s ease, box-shadow 0.2s ease; }
             .memo-input:focus { border-color: #007aff; box-shadow: 0 0 0 2px rgba(0, 122, 255, 0.2); }
             .memo-input:disabled { background-color: #f5f5f5; color: #888; cursor: not-allowed; border-color: #ddd; }
             @keyframes vectorPulse { 0% { box-shadow: 0 0 0 0 rgba(46, 204, 113, 0.4); } 70% { box-shadow: 0 0 0 6px rgba(46, 204, 113, 0); } 100% { box-shadow: 0 0 0 0 rgba(46, 204, 113, 0); } }
             .ai-search-active { border-color: #2ecc71 !important; animation: vectorPulse 2s infinite; background: #eafaf1 !important; }
             .markdown-body { font-size: 0.95rem; color: #333; line-height: 1.6; }
             .markdown-body p { margin-top: 0; margin-bottom: 12px; }
             .markdown-body pre { background: #282c34; color: #abb2bf; padding: 14px; border-radius: 8px; overflow-x: auto; font-family: monospace; font-size: 0.9em; margin-bottom: 12px; }
             .markdown-body code { background: #f0f0f0; color: #e01e5a; padding: 3px 6px; border-radius: 4px; font-family: monospace; font-size: 0.85em; }
             .markdown-body pre code { background: none; color: inherit; padding: 0; }
             .markdown-body blockquote { border-left: 4px solid #007aff; padding-left: 14px; color: #666; margin: 0 0 12px 0; background: #f8f9fa; padding: 8px 14px; border-radius: 0 6px 6px 0; }
             .markdown-body ul, .markdown-body ol { margin-top: 0; margin-bottom: 12px; padding-left: 24px; }
             
             iframe.goog-te-banner-frame { display: none !important; }
             .skiptranslate > iframe { display: none !important; }
             body, html { top: 0px !important; position: static !important; }
             #goog-gt-tt, .goog-te-balloon-frame { display: none !important; }
             .goog-text-highlight { background-color: transparent !important; box-shadow: none !important; }
             
             .goog-logo-link { display: none !important; }
             .goog-te-gadget { color: transparent !important; font-size: 0px; }
             .goog-te-gadget .goog-te-combo { font-size: 0.85rem; padding: 6px 12px; border-radius: 8px; border: 1px solid #e2e8f0; outline: none; background-color: #fff; color: #333; margin: 0; cursor: pointer; font-family: inherit; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1); font-weight: 600; transition: all 0.2s ease; }
             .goog-te-gadget .goog-te-combo:hover { border-color: #cbd5e1; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.15); }"
        </style>
        
        <main style="max-width: 900px; margin: 0 auto; height: 100vh; display: flex; flex-direction: column; background: #fafafa; font-family: -apple-system, sans-serif;">
            
            <div id="google_translate_element" style="position: fixed; top: 16px; right: 24px; z-index: 9999;"></div>

            <header style="padding: 16px 24px; background: #fff; border-bottom: 1px solid #eee; display: flex; flex-direction: column; gap: 16px; z-index: 10;">
                <div style="display: flex; justify-content: space-between; align-items: center;">
                    <div style="display: flex; align-items: center; gap: 12px; flex-wrap: wrap;">
                        <h1 style="margin: 0; font-size: 1.2rem; color: #111; font-weight: 800;">"🧠 AI Memo Vault"</h1>
                        
                        <Show when=move || store.app_status.get() == AppStatus::Ready>
                            <div style="display: flex; gap: 6px; margin-left: 8px; animation: fadeIn 0.3s;">
                                <button class="btn-hover" on:click=move |_| store.export_data() style="padding: 4px 8px; font-size: 0.8rem; background: #f5f5f5; color: #333; border: 1px solid #ddd; border-radius: 4px; cursor: pointer; font-weight: bold;">"💾 백업"</button>
                                <button class="btn-hover" on:click=trigger_import style="padding: 4px 8px; font-size: 0.8rem; background: #f5f5f5; color: #333; border: 1px solid #ddd; border-radius: 4px; cursor: pointer; font-weight: bold;">"📂 복원"</button>
                                <input node_ref=file_input_ref type="file" accept=".json" style="display: none;" on:change=on_file_change />
                            </div>
                        </Show>
                    </div>

                    <div style="display: flex; align-items: center; gap: 16px;">
                        <div style="font-size: 0.85rem; font-weight: bold; color: #007aff;">
                            {move || match store.app_status.get() {
                                AppStatus::NotLoaded => "엔진 대기중".to_string(),
                                AppStatus::Loading => "🚀 AI 엔진 적재 중...".to_string(),
                                AppStatus::Ready => "🟢 AI 엔진 활성화".to_string(),
                                AppStatus::Error(err) => format!("🔴 {}", err),
                            }}
                        </div>
                    </div>
                </div>
                
                <Show when=move || store.app_status.get() == AppStatus::Ready>
                    <div style="display: flex; flex-direction: column; gap: 12px; animation: fadeIn 0.3s;">
                        <div style="display: flex; gap: 8px;">
                            <button class="btn-hover" on:click=move |_| store.set_show_category_manager.set(!store.show_category_manager.get()) style="padding: 4px 8px; font-size: 0.8rem; background: #e3f2fd; color: #1565c0; border: 1px solid #bbdefb; border-radius: 4px; cursor: pointer; font-weight: bold;">"⚙️ 분류 관리"</button>
                            <button class="btn-hover" on:click=move |_| store.clear_memos() style="padding: 4px 8px; font-size: 0.8rem; background: #ffebee; color: #c62828; border: 1px solid #ffcdd2; border-radius: 4px; cursor: pointer; font-weight: bold;">"전체 삭제 🗑️"</button>
                        </div>

                        <div style="display: flex; gap: 8px; width: 100%;">
                            <div style="position: relative; flex: 1;">
                                <span style="position: absolute; left: 12px; top: 10px; font-size: 1rem;">"🔍"</span>
                                <input 
                                    node_ref=store.search_input_ref
                                    type="text"
                                    class=move || if store.ai_search_results.get().is_some() { "search-bar ai-search-active notranslate" } else { "search-bar notranslate" }
                                    placeholder="키워드 또는 문장으로 검색해 보세요. (예: 지난주에 메모한 넷플릭스 구독료가 얼마지?)"
                                    on:input=move |ev| {
                                        store.set_search_query.set(event_target_value(&ev));
                                        if store.ai_search_results.get().is_some() { store.set_ai_search_results.set(None); }
                                    }
                                    style="width: 100%; padding: 10px 10px 10px 38px; border: 1px solid #ddd; border-radius: 8px; font-size: 0.95rem; outline: none; box-sizing: border-box; transition: all 0.3s ease;"
                                />
                                <Show when=move || !store.search_query.get().is_empty()>
                                    <button on:click=move |_| store.clear_search() style="position: absolute; right: 12px; top: 8px; background: none; border: none; cursor: pointer; color: #999; font-size: 1rem;">"✕"</button>
                                </Show>
                            </div>
                            
                            <button 
                                class="btn-hover"
                                on:click=move |_| store.trigger_vector_search()
                                disabled=move || store.is_ai_searching.get() || store.search_query.get().trim().is_empty()
                                style="padding: 0 16px; background: #2ecc71; color: #fff; border: none; border-radius: 8px; cursor: pointer; font-weight: bold; white-space: nowrap; box-shadow: 0 2px 4px rgba(46, 204, 113, 0.3);"
                            >
                                {move || if store.is_ai_searching.get() { "탐색 중... ⏳".to_string() } else { "🧭 자연어 검색".to_string() }}
                            </button>
                        </div>
                        <Show when=move || !store.ai_search_status.get().is_empty()>
                            <div style="font-size: 0.75rem; color: #27ae60; text-align: right; margin-top: -8px;">
                                {move || store.ai_search_status.get()}
                            </div>
                        </Show>
                    </div>
                </Show>
            </header>

            <div style="flex: 1; overflow-y: auto; padding: 24px; display: flex; flex-direction: column; gap: 32px;">
                
                <Show when=move || { let is_ready = store.app_status.get() == AppStatus::Ready; let show_manager = store.show_category_manager.get(); is_ready && show_manager }>
                    <div style="background: #fff; border-radius: 12px; padding: 16px; box-shadow: 0 2px 10px rgba(0,0,0,0.05); border: 1px solid #eaeaea; animation: fadeIn 0.3s;">
                        <h3 style="margin-top: 0; margin-bottom: 12px; font-size: 1rem; color: #333;">"현재 활성화된 카테고리"</h3>
                        <div style="display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 16px;">
                            <For each=move || store.category_list.get().into_iter().filter(|c| c != "기타" && c != "미분류") key=|c| c.clone()
                                children=move |cat| {
                                    let cat_display = cat.clone(); let cat_for_edit = cat.clone(); let cat_for_delete = cat.clone();
                                    let on_edit = move |_| {
                                        let current_cat = cat_for_edit.clone();
                                        if let Some(window) = web_sys::window() {
                                            if let Ok(Some(new_name)) = window.prompt_with_message_and_default("✏️ 새로운 카테고리 이름을 입력하세요:", &current_cat) {
                                                let new_name = new_name.trim().to_string();
                                                if !new_name.is_empty() && new_name != current_cat && !store.category_list.get().contains(&new_name) {
                                                    store.set_category_list.update(|list| { if let Some(pos) = list.iter().position(|x| *x == current_cat) { list[pos] = new_name.clone(); } });
                                                    store.set_memo_list.update(|list| { for m in list.iter_mut() { if m.data.category == current_cat { m.data.category = new_name.clone(); } } });
                                                }
                                            }
                                        }
                                    };
                                    // 👉 사용하지 않는 window 변수 할당 제거 (Warning 해결)
                                    let on_delete = move |_| {
                                        let current_cat = cat_for_delete.clone();
                                        store.set_category_list.update(|list| list.retain(|c| c != &current_cat));
                                        store.set_memo_list.update(|list| { for m in list.iter_mut() { if m.data.category == current_cat { m.data.category = "기타".to_string(); } } });
                                    };
                                    view! {
                                        <span class="notranslate" style="display: inline-flex; align-items: center; gap: 6px; padding: 6px 12px; background: #f0f0f0; border-radius: 20px; font-size: 0.85rem; color: #333; font-weight: 500;">
                                            {cat_display}
                                            <div style="display: flex; gap: 4px; margin-left: 4px; border-left: 1px solid #ccc; padding-left: 6px;">
                                                <button on:click=on_edit title="이름 변경" style="background: none; border: none; cursor: pointer; color: #666; padding: 0; font-size: 0.9rem;">"✏️"</button>
                                                <button on:click=on_delete title="카테고리 삭제" style="background: none; border: none; cursor: pointer; color: #d32f2f; padding: 0; font-size: 0.9rem;">"✕"</button>
                                            </div>
                                        </span>
                                    }
                                }
                            />
                        </div>
                        <div style="display: flex; gap: 8px;">
                            <input class="notranslate" node_ref=category_add_input_ref type="text" placeholder="새 카테고리 이름 입력" on:input=move |ev| store.set_new_category_name.set(event_target_value(&ev)) style="flex: 1; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; outline: none; font-size: 0.95rem;" />
                            <button class="btn-hover" on:click=move |_| { let new_cat = store.new_category_name.get().trim().to_string(); if !new_cat.is_empty() && !store.category_list.get().contains(&new_cat) { store.set_category_list.update(|list| { list.insert(list.len().saturating_sub(2), new_cat); }); store.set_new_category_name.set(String::new()); if let Some(input) = category_add_input_ref.get() { input.set_value(""); } } } style="padding: 10px 20px; background: #222; color: #fff; border: none; border-radius: 6px; cursor: pointer; font-weight: bold;">"추가"</button>
                        </div>
                    </div>
                </Show>

                <Show when=move || store.app_status.get() == AppStatus::NotLoaded>
                    <div style="background: #fff; border-radius: 16px; padding: 40px; box-shadow: 0 10px 30px rgba(0,0,0,0.08); border: 1px solid #eaeaea; max-width: 700px; margin: 40px auto; animation: fadeIn 0.6s cubic-bezier(0.2, 0.8, 0.2, 1);">
                        <div style="text-align: center; margin-bottom: 32px;">
                            <span style="font-size: 3rem; display: block; margin-bottom: 16px;">"🧭"</span>
                            <h2 style="margin: 0; color: #111; font-size: 1.8rem; font-weight: 800;">"AI Memo Vault"</h2>
                            <p style="color: #666; font-size: 1rem; margin-top: 12px;">"내 기기에서 완벽하게 구동되는 로컬 온디바이스 AI 메모장"</p>
                        </div>
                        
                        <div style="text-align: center; margin-bottom: 32px;">
                            {
                                if is_mobile_device() {
                                    leptos::either::Either::Left(view! {
                                        <div style="background: #fff3cd; border: 1px solid #ffe69c; border-radius: 8px; padding: 20px; color: #664d03; display: inline-block; text-align: left; max-width: 90%; box-shadow: 0 4px 6px rgba(0,0,0,0.05);">
                                            <h4 style="margin: 0 0 8px 0; font-weight: 800; font-size: 1.1rem;">"📱 모바일 환경 제한 안내"</h4>
                                            <p style="margin: 0; line-height: 1.5; font-size: 0.95rem;">
                                                "로컬 AI 모델의 크기가 방대하여 모바일 브라우저에서는 메모리 부족(OOM)으로 앱이 종료될 수 있습니다.<br/><br/>원활한 이용을 위해 가급적 <b>PC(데스크탑/노트북) 환경</b>에서 접속해 주세요."
                                            </p>
                                        </div>
                                    })
                                } else {
                                    leptos::either::Either::Right(view! {
                                        <button class="btn-hover" style="padding: 16px 40px; font-size: 1.15rem; background: #000; color: #fff; border: none; border-radius: 8px; cursor: pointer; font-weight: 800; box-shadow: 0 4px 15px rgba(0,0,0,0.2);" on:click=move |_| store.load_model()>
                                            "AI 엔진 적재하기"
                                        </button>
                                    })
                                }
                            }
                        </div>

                        <div style="background: #f0fdf4; border: 1px solid #bbf7d0; border-radius: 10px; padding: 20px; text-align: left; margin-bottom: 16px; box-shadow: 0 2px 8px rgba(34, 197, 94, 0.05);">
                            <h4 style="margin: 0 0 8px 0; font-weight: 800; color: #15803d; font-size: 1.05rem; display: flex; align-items: center; gap: 6px;">
                                "🛡️ 100% 로컬 구동 (서버 전송 무)"
                            </h4>
                            <p style="margin: 0; line-height: 1.6; color: #166534; font-size: 0.95rem;">
                                "이 앱의 인공지능 모델은 " <b>"오직 귀하의 기기 내부에서만"</b> " 실행됩니다. 작성하신 모든 메모와 데이터는 " 
                                <strong style="color: #14532d; background: #dcfce7; padding: 2px 6px; border-radius: 4px; border: 1px solid #bbf7d0;">"단 1바이트도 외부 서버나 클라우드로 전송되지 않으므로"</strong> 
                                ", 개인적인 금융 내역이나 기밀 업무 내용을 완벽하게 안심하고 작성하셔도 됩니다."
                            </p>
                            <p style="margin: 12px 0 0 0; line-height: 1.6; color: #166534; font-size: 0.95rem; border-top: 1px dashed #bbf7d0; padding-top: 12px;">
                                "📌 " <b>"기기 간 연동 불가 안내:"</b> " 클라우드 서버를 사용하지 않기 때문에, " <b>"현재 작업 중인 바로 이 PC(또는 스마트폰)의 브라우저에서만"</b> " 저장된 데이터를 볼 수 있습니다. 다른 기기에서 접속하면 연동되지 않은 빈 화면이 나타납니다."
                            </p>
                        </div>

                        <div style="background: #fff3cd; border: 1px solid #ffe69c; border-radius: 10px; padding: 20px; text-align: left; font-size: 0.9rem; color: #664d03; display: flex; flex-direction: column; gap: 16px;">
                            <div>
                                <h4 style="margin: 0 0 8px 0; font-weight: 800; color: #b02a37; font-size: 1.05rem;">"🚨 데이터 소실 및 공용 기기 주의사항"</h4>
                                <ul style="margin: 0; padding-left: 20px; line-height: 1.6; color: #842029;">
                                    <li>"모든 데이터는 브라우저 내부 스토리지에 저장되므로 " <b>"절대 공용 PC나 타인의 기기에서 사용하지 마십시오."</b></li>
                                    <li>"브라우저 설정에서 " <b>"'캐시 및 사이트 데이터(인터넷 사용 기록)'"</b> "를 지우면 메모와 모델이 모두 삭제됩니다."</li>
                                    <li><b>"시크릿 모드(프라이빗 브라우징)"</b> "에서 실행 후 창을 닫으면 데이터가 영구 삭제됩니다."</li>
                                    <li>"기기의 " <b>"저장 용량이 부족"</b> "할 경우, 브라우저 정책에 의해 다운로드된 AI 모델 파일이 임의로 정리될 수 있습니다."</li>
                                    <li>"소중한 메모 보호를 위해 정기적으로 우측 상단의 " <b>"[💾 백업]"</b> " 버튼을 눌러 파일로 보관해 주세요."</li>
                                </ul>
                            </div>

                            <div style="border-top: 1px dashed #e5cfa4; padding-top: 16px;">
                                <h4 style="margin: 0 0 8px 0; font-weight: 800; color: #664d03; font-size: 1rem;">"📜 오픈소스 AI 모델 라이선스 고지"</h4>
                                <ul style="margin: 0; padding-left: 20px; line-height: 1.5;">
                                    <li><strong>"LLM (분류):"</strong> " Meta Llama 3.2 1B Instruct (Llama 3.2 Community License)"</li>
                                    <li><strong>"Vector (검색):"</strong> " paraphrase-multilingual-MiniLM-L12-v2 (Apache 2.0 License)"</li>
                                </ul>
                            </div>
                        </div>
                    </div>
                </Show>

                <Show when=move || store.app_status.get() == AppStatus::Loading>
                    <div style="margin: auto; width: 100%; max-width: 400px; text-align: center; animation: fadeIn 0.5s;">
                        <h3 style="color: #333; margin-bottom: 16px;">"AI 엔진 적재 중..."</h3>
                        <div style="width: 100%; background: #e1e4e8; border-radius: 8px; height: 10px; overflow: hidden; margin-bottom: 12px;">
                            <div style=move || format!("width: {:.1}%; height: 100%; background: #007aff; transition: width 0.3s ease;", store.worker_progress.get() * 100.0)></div>
                        </div>
                        <div style="font-size: 0.85rem; color: #666;">{move || store.worker_status_msg.get()}</div>
                    </div>
                </Show>

                <Show when=move || store.app_status.get() == AppStatus::Ready>
                    <div style="background: #fff; border-radius: 12px; padding: 16px; box-shadow: 0 2px 10px rgba(0,0,0,0.05); border: 1px solid #eaeaea;">
                        <textarea node_ref=store.memo_input_ref class="memo-input notranslate" style="width: 100%; min-height: 100px; padding: 12px; border: 1px solid #ddd; border-radius: 8px; resize: none; overflow: hidden; font-size: 1rem; outline: none; font-family: inherit; box-sizing: border-box;" placeholder="아무 생각이나 코드, 일상, 에러 로그를 던져주세요. AI가 알아서 분류합니다." on:input=move |ev| { let target = ev.target().unwrap().dyn_into::<web_sys::HtmlTextAreaElement>().unwrap(); let style = web_sys::HtmlElement::style(&target); let _ = style.set_property("height", "auto"); let _ = style.set_property("height", &format!("{}px", target.scroll_height())); store.set_input_text.set(target.value()); } disabled=move || store.is_generating.get()></textarea>
                        <div style="display: flex; justify-content: flex-end; margin-top: 12px;">
                            <button class="btn-hover" style="padding: 10px 24px; background: #007aff; color: #fff; border: none; border-radius: 6px; font-weight: 600; cursor: pointer;" disabled=move || store.is_generating.get() || store.input_text.get().trim().is_empty() on:click=move |_| store.analyze_memo()> {move || if store.is_generating.get() { "정밀 분류 및 데이터 추출 중... ⏳".to_string() } else { "분류해서 적재하기 📥".to_string() }}</button>
                        </div>
                    </div>

                    <Show when=move || store.ai_search_results.get().is_some()>
                        <div style="background: #eafaf1; border: 1px solid #a9dfbf; border-radius: 8px; padding: 12px 16px; color: #27ae60; font-size: 0.9rem; display: flex; align-items: center; justify-content: space-between; animation: fadeIn 0.3s;">
                            <span><b>"🧭 쾌속 검색 완료:"</b> " 관련 메모를 찾아왔습니다."</span>
                        </div>
                    </Show>

                    <div style="display: flex; flex-direction: column; gap: 32px;">
                        <For each=move || store.grouped_memos() key=|(cat, items)| { let t_sum: f64 = items.iter().map(|m| m.timestamp).sum(); format!("{}-{}-{}", cat, items.len(), t_sum) }
                            children=move |(category, items)| {
                                let cat_name = category.clone();
                                
                                let total_amount: i32 = items.iter().filter_map(|m| m.data.amount).sum();
                                
                                let format_num = |n: i32| -> String {
                                    let s = n.to_string();
                                    let mut result = String::new();
                                    let mut count = 0;
                                    for c in s.chars().rev() {
                                        if count != 0 && count % 3 == 0 { result.push(','); }
                                        result.push(c);
                                        count += 1;
                                    }
                                    result.chars().rev().collect()
                                };

                                view! {
                                    <div style="display: flex; flex-direction: column; gap: 16px; animation: fadeIn 0.4s ease-out;">
                                        <div style="display: flex; align-items: center; gap: 8px; border-bottom: 2px solid #eee; padding-bottom: 8px;">
                                            <h2 class="notranslate" style="margin: 0; font-size: 1.3rem; color: #222;">{cat_name.clone()}</h2>
                                            <span style="background: #f0f0f0; color: #666; font-size: 0.8rem; padding: 2px 8px; border-radius: 12px; font-weight: 700;">{items.len()}</span>
                                            
                                            <Show when=move || { total_amount > 0 }>
                                                <span style="margin-left: auto; color: #e74c3c; font-weight: 800; font-size: 1.1rem; background: #fdf2e9; padding: 4px 12px; border-radius: 8px; border: 1px solid #fadbd8; box-shadow: 0 2px 4px rgba(231, 76, 60, 0.1);">
                                                    {format!("총 지출: ₩ {}", format_num(total_amount))}
                                                </span>
                                            </Show>
                                        </div>
                                        <div style="display: grid; grid-template-columns: 1fr; gap: 16px;">
                                            <For each=move || items.clone() key=|memo| format!("{}-{}", memo.id, memo.timestamp)
                                                children=move |memo| {
                                                    let memo_id = memo.id; let memo_store = StoredValue::new(memo.clone());
                                                    let (is_editing, set_is_editing) = signal(false);
                                                    let (edit_content, set_edit_content) = signal(memo.data.content.clone());
                                                    view! {
                                                        <div class="memo-card" style="background: #fff; border-radius: 12px; padding: 20px; box-shadow: 0 1px 4px rgba(0,0,0,0.08); border: 1px solid #eaeaea; position: relative;">
                                                            <Show when=move || is_editing.get()
                                                                fallback=move || {
                                                                    view! {
                                                                        <div>
                                                                            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; border-bottom: 1px dashed #eee; padding-bottom: 8px;">
                                                                                <span style="font-size: 0.8rem; color: #888; font-weight: 500;">{memo_store.with_value(|m| m.date_str.clone())}</span>
                                                                                <div style="display: flex; gap: 4px;">
                                                                                    <button class="magic-wand" title="직접 수정" on:click=move |_| { memo_store.with_value(|m| { set_edit_content.set(m.data.content.clone()); set_is_editing.set(true); }); } style="background: #f9f9f9; border: 1px solid #ddd; border-radius: 4px; cursor: pointer; padding: 2px 6px; font-size: 0.8rem;">"✏️"</button>
                                                                                    <button class="magic-wand" title="삭제" on:click=move |_| store.delete_memo(memo_id) style="background: #fff0f0; border: 1px solid #ffcdd2; border-radius: 4px; cursor: pointer; padding: 2px 6px; font-size: 0.8rem; color: #d32f2f;">"🗑️"</button>
                                                                                </div>
                                                                            </div>
                                                                            
                                                                            <Show when=move || store.search_query.get().trim().is_empty() fallback=move || { view! { <div class="notranslate" style="font-size: 0.95rem; color: #444; line-height: 1.6; white-space: pre-wrap;"><HighlightText text=memo_store.with_value(|m| m.data.content.clone()) query=store.search_query /></div> } }>
                                                                                { move || { let mut options = Options::empty(); options.insert(Options::ENABLE_TABLES); options.insert(Options::ENABLE_STRIKETHROUGH); let content = memo_store.with_value(|m| m.data.content.clone()); let parser = Parser::new_ext(&content, options); let mut parsed_md_html = String::new(); html::push_html(&mut parsed_md_html, parser); view! { <div class="markdown-body notranslate" inner_html=parsed_md_html></div> } }}
                                                                            </Show>
                                                                        </div>
                                                                    }
                                                                }
                                                            >
                                                                <div style="display: flex; flex-direction: column; gap: 12px; width: 100%; animation: fadeIn 0.2s;">
                                                                    <textarea class="notranslate" prop:value=move || edit_content.get() on:input=move |ev| set_edit_content.set(event_target_value(&ev)) style="width: 100%; min-height: 150px; padding: 12px; border: 1px solid #007aff; border-radius: 6px; resize: vertical; font-size: 0.95rem; outline: none; font-family: inherit;"></textarea>
                                                                    <div style="display: flex; justify-content: flex-end; gap: 8px;">
                                                                        <button class="btn-hover" on:click=move |_| set_is_editing.set(false) style="padding: 8px 16px; background: #f5f5f5; border: 1px solid #ddd; border-radius: 6px; cursor: pointer;">"취소"</button>
                                                                        <button class="btn-hover" on:click=move |_| { store.set_memo_list.update(|list| { if let Some(m) = list.iter_mut().find(|x| x.id == memo_id) { m.data.content = edit_content.get(); } }); set_is_editing.set(false); } style="padding: 8px 16px; background: #007aff; color: #fff; border: none; border-radius: 6px; cursor: pointer; font-weight: bold;">"💾 저장"</button>
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
                    </div>
                </Show>
            </div>
        </main>
    }
}