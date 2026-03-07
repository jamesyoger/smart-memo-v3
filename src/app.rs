use leptos::prelude::*;
use std::io::Cursor;
use reqwest;
use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use tokenizers::Tokenizer;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use std::rc::Rc;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent, Worker}; 
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub enum WorkerInput {
    LoadModel,
    Prompt(String),
    Stop,
}

#[derive(Serialize, Deserialize)]
pub enum WorkerOutput {
    Status(String),
    Ready,
    Token(String),
    Done,
    Error(String),
}

#[wasm_bindgen(inline_js = r#"
export function yield_event_loop() {
    return new Promise(resolve => setTimeout(resolve, 0));
}

export function spawn_worker() { 
    console.log('🟢 [메인] 1. 워커 생성 함수가 호출되었습니다.');
    
    let js_url = '';
    const links = document.querySelectorAll('link');
    for (let i = 0; i < links.length; i++) {
        if (links[i].href.includes('llama3_worker') && links[i].href.endsWith('.js')) {
            js_url = links[i].href;
            break;
        }
    }
    
    const wasm_url = js_url.replace('.js', '_bg.wasm');
    
    const workerCode = `
        console.log('🟡 [워커] 3. 워커 내부 공간이 열렸습니다. WASM 시동을 시작합니다.');
        import init from '${js_url}';
        
        init('${wasm_url}').then(() => {
            console.log('🟡 [워커] 4. WASM 엔진 시동 완벽 성공! 메인 스레드의 명령을 기다립니다.');
        }).catch(e => {
            console.error('🔴 [워커] WASM 시동 치명적 실패:', e);
        });
    `;
    const blob = new Blob([workerCode], { type: 'application/javascript' });
    const worker = new Worker(URL.createObjectURL(blob), { type: 'module' });
    return worker;
}
"#)]
extern "C" {
    fn spawn_worker() -> Worker;
    fn yield_event_loop() -> js_sys::Promise;
}

#[derive(Clone, PartialEq)]
enum AppStatus {
    NotLoaded,
    FetchingFiles,
    Ready,
    Error(String),
}

#[derive(Clone)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
}

struct AiState {
    tokenizer: Tokenizer,
    model: ModelWeights,
    index_pos: usize,
}

async fn fetch_local_bytes(path: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::get(path).await.map_err(|e| e.to_string())?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}

pub fn worker_main() {
    let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
    let ai_state: Rc<RefCell<Option<AiState>>> = Rc::new(RefCell::new(None));
    let stop_signal = Rc::new(RefCell::new(false)); 
    
    let send_output = {
        let global = global.clone();
        move |output: WorkerOutput| {
            let json = serde_json::to_string(&output).unwrap();
            global.post_message(&JsValue::from_str(&json)).unwrap();
        }
    };

    let global_for_closure = global.clone(); 
    let stop_signal_for_msg = stop_signal.clone();

    let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
        let data = e.data().as_string().unwrap_or_default();
        let input: WorkerInput = match serde_json::from_str(&data) {
            Ok(val) => val,
            Err(_) => return,
        };
        
        if let WorkerInput::Stop = input {
            *stop_signal_for_msg.borrow_mut() = true;
            return;
        }

        let ai_state = ai_state.clone();
        let send_output = send_output.clone();
        let async_global = global_for_closure.clone(); 
        let stop_signal_for_async = stop_signal.clone();
        
        match input {
            WorkerInput::LoadModel => {
                wasm_bindgen_futures::spawn_local(async move {
                    send_output(WorkerOutput::Status("⏳ [단계 1/5] 토크나이저 다운로드 중...".into()));
                    let origin = async_global.location().origin(); 
                    
                    let tokenizer_url = format!("{}/tokenizer.json", origin);
                    let model_url = format!("{}/model.gguf", origin);

                    let tokenizer_bytes = match fetch_local_bytes(&tokenizer_url).await {
                        Ok(b) => b,
                        Err(e) => { send_output(WorkerOutput::Error(format!("토크나이저 다운로드 실패: {}", e))); return; }
                    };

                    send_output(WorkerOutput::Status("⏳ [단계 2/5] 모델 파일 다운로드 중...".into()));
                    let model_bytes = match fetch_local_bytes(&model_url).await {
                        Ok(b) => b,
                        Err(e) => { send_output(WorkerOutput::Error(format!("모델 다운로드 실패: {}", e))); return; }
                    };

                    send_output(WorkerOutput::Status("⚙️ [단계 3/5] 토크나이저 파싱 시작...".into()));
                    let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes).expect("Tokenizer error");

                    send_output(WorkerOutput::Status("⚙️ [단계 4/5] GGUF 메타데이터 읽기 시작...".into()));
                    let mut cursor = Cursor::new(&model_bytes);
                    let gguf_content = gguf_file::Content::read(&mut cursor).expect("GGUF read error");

                    send_output(WorkerOutput::Status("🔥 [단계 5/5] 인공지능 신경망 조립 시작...".into()));
                    let model = match ModelWeights::from_gguf(gguf_content, &mut cursor, &Device::Cpu) {
                        Ok(m) => m,
                        Err(e) => { send_output(WorkerOutput::Error(format!("조립 실패: {:?}", e))); return; }
                    };

                    *ai_state.borrow_mut() = Some(AiState { tokenizer, model, index_pos: 0 });
                    send_output(WorkerOutput::Ready);
                });
            }
            WorkerInput::Prompt(prompt) => {
                wasm_bindgen_futures::spawn_local(async move {
                    *stop_signal_for_async.borrow_mut() = false;

                    let mut all_tokens = Vec::new();
                    let mut next_token_id = 0;
                    let mut logits_processor = LogitsProcessor::new(1337, Some(0.3), Some(0.85));

                    // [핵심 해결] 자물쇠(borrow_mut)를 쓰는 구간을 중괄호 { } 로 완전히 격리합니다.
                    {
                        let mut state_ref = ai_state.borrow_mut();
                        let state = state_ref.as_mut().unwrap();

                        let formatted_prompt = if state.index_pos == 0 {
                            format!("<|start_header_id|>system<|end_header_id|>\n\n당신은 친절하고 뛰어난 문서 요약 AI입니다. 사용자가 입력한 내용을 분석하여, 가장 중요한 핵심 내용만 정확히 3줄로 요약해서 한국어로 대답하세요. 절대로 인사말이나 불필요한 설명을 덧붙이지 마세요.<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n", prompt)
                        } else {
                            format!("<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n", prompt)
                        };
                        
                        let tokens = state.tokenizer.encode(formatted_prompt, state.index_pos == 0).unwrap();
                        let prompt_tokens = tokens.get_ids().to_vec();
                        
                        all_tokens = prompt_tokens.clone();

                        if state.index_pos == 0 {
                            let input_tensor = Tensor::new(prompt_tokens.as_slice(), &Device::Cpu).unwrap().unsqueeze(0).unwrap();
                            let logits = state.model.forward(&input_tensor, state.index_pos).unwrap();
                            state.index_pos += prompt_tokens.len();
                            
                            let logits = logits.squeeze(0).unwrap();
                            next_token_id = logits_processor.sample(&logits).unwrap();
                            all_tokens.push(next_token_id);
                        } else {
                            for (i, &token) in prompt_tokens.iter().enumerate() {
                                let input_tensor = Tensor::new(&[token], &Device::Cpu).unwrap().unsqueeze(0).unwrap();
                                let logits = state.model.forward(&input_tensor, state.index_pos).unwrap();
                                state.index_pos += 1;

                                if i == prompt_tokens.len() - 1 {
                                    let logits = logits.squeeze(0).unwrap();
                                    next_token_id = logits_processor.sample(&logits).unwrap();
                                    all_tokens.push(next_token_id);
                                }
                            }
                        }
                    } // 💡 여기서 자물쇠가 안전하게 해제됩니다!

                    if next_token_id != 128001 && next_token_id != 128009 {
                        let mut generated_ids = vec![next_token_id];
                        
                        let decoded_text = {
                            let state_ref = ai_state.borrow();
                            let state = state_ref.as_ref().unwrap();
                            state.tokenizer.decode(&generated_ids, true).unwrap_or_default()
                        };
                        send_output(WorkerOutput::Token(decoded_text));

                        for _ in 0..400 { 
                            if *stop_signal_for_async.borrow() {
                                break;
                            }

                            // [핵심 해결] 1글자를 뽑을 때마다 아주 잠깐만 자물쇠를 채우고 즉시 풉니다.
                            let new_logits = {
                                let mut state_ref = ai_state.borrow_mut();
                                let state = state_ref.as_mut().unwrap();
                                
                                let input_tensor = Tensor::new(&[next_token_id], &Device::Cpu).unwrap().unsqueeze(0).unwrap();
                                let logits = state.model.forward(&input_tensor, state.index_pos).unwrap();
                                state.index_pos += 1;
                                logits.squeeze(0).unwrap()
                            }; // 💡 여기서 다시 자물쇠 해제!

                            let start_at = all_tokens.len().saturating_sub(64);
                            let penalized_logits = candle_transformers::utils::apply_repeat_penalty(
                                &new_logits,
                                1.15, 
                                &all_tokens[start_at..],
                            ).unwrap_or(new_logits);

                            next_token_id = logits_processor.sample(&penalized_logits).unwrap();
                            
                            if next_token_id == 128001 || next_token_id == 128009 { break; }

                            all_tokens.push(next_token_id);
                            generated_ids.push(next_token_id);
                            
                            let decoded_text = {
                                let state_ref = ai_state.borrow();
                                let state = state_ref.as_ref().unwrap();
                                state.tokenizer.decode(&generated_ids, true).unwrap_or_default()
                            };
                            send_output(WorkerOutput::Token(decoded_text));

                            // 💡 자물쇠가 완전히 풀린 안전한 상태에서 숨을 고릅니다.
                            let _ = wasm_bindgen_futures::JsFuture::from(yield_event_loop()).await;
                        }
                    }

                    send_output(WorkerOutput::Done);
                });
            }
            _ => {} 
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    
    global.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
}

#[component]
pub fn App() -> impl IntoView {
    let app_status = RwSignal::new(AppStatus::NotLoaded);
    let chat_log = RwSignal::new(Vec::<ChatMessage>::new());
    let input_text = RwSignal::new(String::new());
    let is_generating = RwSignal::new(false);
    let worker_status_msg = RwSignal::new(String::new());

    let worker = spawn_worker();

    let onmessage = {
        let app_status = app_status.clone();
        let chat_log = chat_log.clone();
        let is_generating = is_generating.clone();
        let worker_status_msg = worker_status_msg.clone();

        Closure::wrap(Box::new(move |e: MessageEvent| {
            let data = e.data().as_string().unwrap();
            let output: WorkerOutput = serde_json::from_str(&data).unwrap();
            
            match output {
                WorkerOutput::Status(msg) => worker_status_msg.set(msg),
                WorkerOutput::Ready => app_status.set(AppStatus::Ready),
                WorkerOutput::Token(text) => {
                    chat_log.update(|log| {
                        if let Some(msg) = log.last_mut() {
                            if msg.sender == "AI" { msg.content = text; }
                        }
                    });
                }
                WorkerOutput::Done => is_generating.set(false),
                WorkerOutput::Error(err) => {
                    web_sys::console::log_1(&JsValue::from_str(&format!("🔴 [메인] 워커에서 에러 발생: {}", err)));
                    app_status.set(AppStatus::Error(err));
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>)
    };
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget(); 

    let worker_store = StoredValue::new_local(worker);

    let load_model = move |_| {
        app_status.set(AppStatus::FetchingFiles);
        worker_status_msg.set("⏳ 워커 스레드 시작 중... (F12 콘솔 확인)".to_string());
        
        let input = WorkerInput::LoadModel;
        worker_store.with_value(|w| {
            if let Err(e) = w.post_message(&JsValue::from_str(&serde_json::to_string(&input).unwrap())) {
                app_status.set(AppStatus::Error(format!("워커 통신 실패: {:?}", e)));
            }
        });
    };

    let send_message = move || {
        let prompt_clone = input_text.get();
        if prompt_clone.trim().is_empty() { return; }
        
        is_generating.set(true);
        chat_log.update(|log| log.push(ChatMessage { sender: "나".to_string(), content: prompt_clone.clone() }));
        chat_log.update(|log| log.push(ChatMessage { sender: "AI".to_string(), content: "".to_string() }));
        input_text.set(String::new());

        let input = WorkerInput::Prompt(prompt_clone);
        worker_store.with_value(|w| {
            w.post_message(&JsValue::from_str(&serde_json::to_string(&input).unwrap())).unwrap();
        });
    };

    let stop_generation = move || {
        worker_store.with_value(|w| {
            let input = WorkerInput::Stop;
            w.post_message(&JsValue::from_str(&serde_json::to_string(&input).unwrap())).unwrap();
        });
        is_generating.set(false);
    };

    view! {
        <style>
            "@keyframes pulse { 0% { opacity: 0.4; } 50% { opacity: 1; } 100% { opacity: 0.4; } }"
        </style>
        
        <main style="max-width: 800px; margin: 40px auto; padding: 20px; font-family: sans-serif;">
            <div style="text-align: center; margin-bottom: 30px;">
                <h1 style="color: #1a73e8;">"Llama 3.2 로컬 챗봇 (Web Worker 가속)"</h1>
                
                <Show when=move || app_status.get() == AppStatus::NotLoaded>
                    <button 
                        style="padding: 12px 24px; background: #1a73e8; color: white; border: none; border-radius: 8px; cursor: pointer; font-weight: bold;"
                        on:click=load_model
                    >
                        "🚀 엔진 로드 시작"
                    </button>
                </Show>

                <Show when=move || app_status.get() == AppStatus::FetchingFiles>
                    <div style="color: #666; font-weight: bold; margin-bottom: 10px;">{move || worker_status_msg.get()}</div>
                </Show>

                <Show when=move || matches!(app_status.get(), AppStatus::Error(_))>
                    <div style="background: #ffe6e6; color: #d93025; padding: 15px; border-radius: 8px; border: 1px solid #f2bcba; margin-top: 10px;">
                        <strong>"🚨 오류 발생: "</strong>
                        {move || match app_status.get() {
                            AppStatus::Error(e) => e,
                            _ => String::new()
                        }}
                    </div>
                </Show>
            </div>

            <Show when=move || app_status.get() == AppStatus::Ready>
                <div style="height: 500px; overflow-y: auto; background: #f8f9fa; border-radius: 16px; padding: 20px; display: flex; flex-direction: column; gap: 12px; border: 1px solid #ddd;">
                    <For
                        each=move || chat_log.get()
                        key=|msg| format!("{}{}", msg.sender, msg.content)
                        children=move |msg| {
                            let is_me = msg.sender == "나";
                            view! {
                                <div style=format!("align-self: {}; max-width: 80%;", if is_me { "flex-end" } else { "flex-start" })>
                                    <div style=format!(
                                        "background: {}; color: {}; padding: 10px 15px; border-radius: 18px; box-shadow: 0 2px 4px rgba(0,0,0,0.1);",
                                        if is_me { "#007bff" } else { "#ffffff" },
                                        if is_me { "white" } else { "black" }
                                    )>
                                        <div style="font-size: 0.8rem; font-weight: bold; margin-bottom: 4px;">{msg.sender}</div>
                                        <div style="white-space: pre-wrap; line-height: 1.4;">{msg.content}</div>
                                    </div>
                                </div>
                            }
                        }
                    />
                    
                    <Show when=move || is_generating.get()>
                        <div style="align-self: flex-start; max-width: 80%; background: #e9ecef; color: #495057; padding: 8px 16px; border-radius: 18px; font-size: 0.85rem; font-weight: bold; display: flex; align-items: center; gap: 8px; animation: pulse 1.5s infinite;">
                            "⏳ AI가 답변을 작성 중입니다..."
                        </div>
                    </Show>
                </div>

                <div style="margin-top: 20px; display: flex; gap: 10px; align-items: flex-end;">
                    <textarea 
                        style="flex: 1; padding: 12px; border: 1px solid #ccc; border-radius: 8px; resize: vertical; min-height: 100px; font-family: inherit; line-height: 1.5;"
                        placeholder="메시지를 입력하세요... (Shift+Enter로 줄바꿈, 우측 하단을 드래그해서 크기 조절 가능)"
                        rows="5"
                        prop:value=move || input_text.get()
                        on:input=move |ev| input_text.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" && !ev.shift_key() && !ev.is_composing() {
                                ev.prevent_default(); 
                                if !is_generating.get() { send_message(); }
                            }
                        }
                    ></textarea>
                    
                    <Show when=move || !is_generating.get()>
                        <button 
                            type="button"
                            style="padding: 12px 24px; height: 50px; background: #28a745; color: white; border: none; border-radius: 8px; font-weight: bold; cursor: pointer; min-width: 80px;"
                            on:click=move |_| { send_message(); }
                        >
                            "전송"
                        </button>
                    </Show>
                    
                    <Show when=move || is_generating.get()>
                        <button 
                            type="button"
                            style="padding: 12px 24px; height: 50px; background: #dc3545; color: white; border: none; border-radius: 8px; font-weight: bold; cursor: pointer; min-width: 80px;"
                            on:click=move |_| { stop_generation(); }
                        >
                            "중지 ⏹"
                        </button>
                    </Show>
                </div>
            </Show>
        </main>
    }
}