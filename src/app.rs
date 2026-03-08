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
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent, Worker, HtmlTextAreaElement}; 
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
        import init from '${js_url}';
        init('${wasm_url}').then(() => {
            console.log('🟢 Llama-3.2 엔진 준비 완료');
        });
    `;
    const blob = new Blob([workerCode], { type: 'application/javascript' });
    return new Worker(URL.createObjectURL(blob), { type: 'module' });
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
    pub id: usize,
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
                    send_output(WorkerOutput::Status("⏳ AI 엔진을 로드 중입니다...".into()));
                    let origin = async_global.location().origin(); 
                    let tokenizer_url = format!("{}/tokenizer.json", origin);
                    let model_url = format!("{}/model.gguf", origin);

                    let tokenizer_bytes = match fetch_local_bytes(&tokenizer_url).await {
                        Ok(b) => b,
                        Err(e) => { send_output(WorkerOutput::Error(e)); return; }
                    };
                    let model_bytes = match fetch_local_bytes(&model_url).await {
                        Ok(b) => b,
                        Err(e) => { send_output(WorkerOutput::Error(e)); return; }
                    };

                    let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes).expect("Tokenizer error");
                    let mut cursor = Cursor::new(&model_bytes);
                    let gguf_content = gguf_file::Content::read(&mut cursor).expect("GGUF read error");
                    let model = ModelWeights::from_gguf(gguf_content, &mut cursor, &Device::Cpu).expect("Model error");

                    *ai_state.borrow_mut() = Some(AiState { tokenizer, model, index_pos: 0 });
                    send_output(WorkerOutput::Ready);
                });
            }
            WorkerInput::Prompt(prompt) => {
                wasm_bindgen_futures::spawn_local(async move {
                    *stop_signal_for_async.borrow_mut() = false;
                    
                    let mut all_tokens;
                    let mut next_token_id;
                    let mut logits_processor = LogitsProcessor::new(1337, Some(0.4), Some(0.9));

                    {
                        let mut state_ref = ai_state.borrow_mut();
                        let state = match state_ref.as_mut() {
                            Some(s) => s,
                            None => {
                                send_output(WorkerOutput::Error("엔진이 아직 로드되지 않았습니다.".to_string()));
                                return;
                            }
                        };

                        let formatted_prompt = if state.index_pos == 0 {
                            format!("<|start_header_id|>system<|end_header_id|>\n\n당신은 친절한 인공지능 개인 비서입니다. 한국어로 답변하세요.<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n", prompt)
                        } else {
                            format!("<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n", prompt)
                        };
                        
                        let tokens = state.tokenizer.encode(formatted_prompt, state.index_pos == 0).unwrap();
                        let prompt_tokens = tokens.get_ids().to_vec();
                        all_tokens = prompt_tokens.clone();

                        let mut final_logits = None;
                        
                        if state.index_pos == 0 {
                            let input_tensor = Tensor::new(prompt_tokens.as_slice(), &Device::Cpu).unwrap().unsqueeze(0).unwrap();
                            final_logits = Some(state.model.forward(&input_tensor, state.index_pos).unwrap());
                            state.index_pos += prompt_tokens.len();
                        } else {
                            for &token in prompt_tokens.iter() {
                                let input_tensor = Tensor::new(&[token], &Device::Cpu).unwrap().unsqueeze(0).unwrap();
                                final_logits = Some(state.model.forward(&input_tensor, state.index_pos).unwrap());
                                state.index_pos += 1;
                            }
                        }
                        
                        let logits = final_logits.unwrap().squeeze(0).unwrap();
                        next_token_id = logits_processor.sample(&logits).unwrap();
                        all_tokens.push(next_token_id);
                    }

                    if next_token_id != 128001 && next_token_id != 128009 {
                        let mut generated_ids = vec![next_token_id];
                        for _ in 0..1000 { 
                            if *stop_signal_for_async.borrow() { break; }

                            let new_logits = {
                                let mut state_ref = ai_state.borrow_mut();
                                let state = state_ref.as_mut().unwrap();
                                let input_tensor = Tensor::new(&[next_token_id], &Device::Cpu).unwrap().unsqueeze(0).unwrap();
                                let logits = state.model.forward(&input_tensor, state.index_pos).unwrap();
                                state.index_pos += 1;
                                logits.squeeze(0).unwrap()
                            };

                            let start_at = all_tokens.len().saturating_sub(64);
                            let penalized_logits = candle_transformers::utils::apply_repeat_penalty(&new_logits, 1.15, &all_tokens[start_at..]).unwrap_or(new_logits);
                            next_token_id = logits_processor.sample(&penalized_logits).unwrap();
                            
                            if next_token_id == 128001 || next_token_id == 128009 { break; }

                            all_tokens.push(next_token_id);
                            generated_ids.push(next_token_id);
                            
                            let decoded_text = {
                                let state_ref = ai_state.borrow();
                                let state = state_ref.as_ref().unwrap();
                                state.tokenizer.decode(&generated_ids, true).unwrap_or_default().replace("", "")
                            };
                            send_output(WorkerOutput::Token(decoded_text));
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
    let current_ai_msg = RwSignal::new(String::new()); 

    let worker = spawn_worker();
    let onmessage = {
        let app_status = app_status.clone();
        let chat_log = chat_log.clone();
        let is_generating = is_generating.clone();
        let worker_status_msg = worker_status_msg.clone();
        let current_ai_msg = current_ai_msg.clone();

        Closure::wrap(Box::new(move |e: MessageEvent| {
            let data = e.data().as_string().unwrap();
            let output: WorkerOutput = serde_json::from_str(&data).unwrap();
            match output {
                WorkerOutput::Status(msg) => worker_status_msg.set(msg),
                WorkerOutput::Ready => app_status.set(AppStatus::Ready),
                WorkerOutput::Token(text) => {
                    current_ai_msg.set(text);
                }
                WorkerOutput::Done => {
                    is_generating.set(false);
                    chat_log.update(|log| {
                        log.push(ChatMessage {
                            id: log.len(),
                            sender: "AI".to_string(),
                            content: current_ai_msg.get(),
                        });
                    });
                    current_ai_msg.set(String::new()); 
                }
                WorkerOutput::Error(err) => app_status.set(AppStatus::Error(err)),
            }
        }) as Box<dyn FnMut(MessageEvent)>)
    };
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget(); 

    let worker_store = StoredValue::new_local(worker);

    let load_model = move |_| {
        app_status.set(AppStatus::FetchingFiles);
        let input = WorkerInput::LoadModel;
        worker_store.with_value(|w| {
            w.post_message(&JsValue::from_str(&serde_json::to_string(&input).unwrap())).unwrap();
        });
    };

    let send_message = move || {
        let prompt_clone = input_text.get();
        if prompt_clone.trim().is_empty() { return; }
        
        is_generating.set(true);
        current_ai_msg.set(String::new()); 
        
        chat_log.update(|log| {
            let next_id = log.len();
            log.push(ChatMessage { id: next_id, sender: "나".to_string(), content: prompt_clone.clone() });
        });
        
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
    };

    view! {
        <style>
            "@keyframes fadeIn { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }"
            "@keyframes dotPulse { 0%, 100% { opacity: .2; } 50% { opacity: 1; } }"
            "textarea:disabled { opacity: 0.6; cursor: not-allowed; }"
            "button:disabled { opacity: 0.6; cursor: not-allowed; }"
        </style>
        
        <main style="max-width: 900px; margin: 0 auto; height: 100vh; display: flex; flex-direction: column; background: #ffffff; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;">
            <header style="padding: 16px 20px; border-bottom: 1px solid #eee; display: flex; justify-content: space-between; align-items: center;">
                <h1 style="margin: 0; font-size: 1.1rem; color: #111; font-weight: 700;">"Llama-3.2 Local Edge AI"</h1>
                <div style="font-size: 0.8rem; color: #666; font-weight: 500;">
                    {move || match app_status.get() {
                        AppStatus::NotLoaded => "대기 중".into(),
                        AppStatus::FetchingFiles => worker_status_msg.get(),
                        AppStatus::Ready => "🟢 암호화된 로컬 연결됨".into(),
                        AppStatus::Error(_) => "🔴 오류".into(),
                    }}
                </div>
            </header>

            <div style="flex: 1; overflow-y: auto; padding: 20px; background: #fafafa; display: flex; flex-direction: column; gap: 16px;">
                
                <Show when=move || app_status.get() == AppStatus::NotLoaded>
                    <div style="margin: auto; max-width: 500px; width: 100%; text-align: center; animation: fadeIn 0.5s;">
                        <div style="font-size: 3rem; margin-bottom: 12px;">"🛡️"</div>
                        <h2 style="font-size: 1.5rem; color: #111; margin-bottom: 24px; font-weight: 800;">"안전하고 강력한 Local AI"</h2>
                        
                        <div style="text-align: left; background: #fff; border: 1px solid #eaeaea; border-radius: 16px; padding: 20px; margin-bottom: 24px; box-shadow: 0 4px 6px rgba(0,0,0,0.02);">
                            <div style="margin-bottom: 16px;">
                                <h3 style="font-size: 1rem; color: #222; margin: 0 0 6px 0; display: flex; align-items: center; gap: 6px;">
                                    "⚡ Edge Computing 기술"
                                </h3>
                                <p style="font-size: 0.85rem; color: #555; margin: 0; line-height: 1.5;">
                                    "본 챗봇은 클라우드 서버를 거치지 않습니다. 최신 WebAssembly 기술을 활용하여 사용자 기기(Edge)의 브라우저 내부에서 거대 언어 모델(LLM)을 직접 구동합니다."
                                </p>
                            </div>
                            
                            <div style="margin-bottom: 16px;">
                                <h3 style="font-size: 1rem; color: #222; margin: 0 0 6px 0; display: flex; align-items: center; gap: 6px;">
                                    "🔒 완벽한 프라이버시 보장"
                                </h3>
                                <p style="font-size: 0.85rem; color: #555; margin: 0; line-height: 1.5;">
                                    "귀하가 입력하는 모든 질문과 대화 내용은 "
                                    <strong style="color: #d93025;">"절대 외부 서버나 인터넷으로 전송되지 않습니다."</strong>
                                    " 모든 데이터 처리는 귀하의 하드웨어 내부에서만 안전하게 이루어집니다."
                                </p>
                            </div>
                        </div>

                        // 💡 추가된 Beta/Demo 경고 영역
                        <div style="margin-bottom: 30px; background: #fff8e1; border-left: 4px solid #ffb300; padding: 14px 16px; border-radius: 8px; text-align: left;">
                            <h3 style="font-size: 0.9rem; color: #b28900; margin: 0 0 6px 0; display: flex; align-items: center; gap: 6px;">
                                "⚠️ Beta Preview (데모 버전)"
                            </h3>
                            <p style="font-size: 0.8rem; color: #666; margin: 0; line-height: 1.5;">
                                "현재 이 애플리케이션은 연구 및 개발 목적의 테스트 버전입니다. 아직 개발이 진행 중이므로 속도 저하나 예기치 않은 동작이 발생할 수 있습니다."
                            </p>
                        </div>

                        <button style="width: 100%; padding: 16px; background: #000; color: #fff; border: none; border-radius: 12px; cursor: pointer; font-weight: 700; font-size: 1rem; transition: background 0.2s;" on:click=load_model>
                            "엔진 로드 및 시작하기"
                        </button>
                        
                        <p style="font-size: 0.7rem; color: #888; margin-top: 24px; line-height: 1.6;">
                            "본 애플리케이션은 Meta의 Llama 3.2 모델을 기반으로 작동하며, " <br/>
                            <a href="https://github.com/meta-llama/llama-models/blob/main/models/llama3_2/LICENSE" target="_blank" style="color: #666; text-decoration: underline;">"Llama 3.2 Community License Agreement"</a>
                            "를 엄격히 준수합니다."
                        </p>
                    </div>
                </Show>

                <For
                    each=move || chat_log.get()
                    key=|msg| msg.id 
                    children=move |msg| {
                        let is_me = msg.sender == "나";
                        view! {
                            <div style=format!("display: flex; flex-direction: column; align-items: {}; animation: fadeIn 0.3s forwards;", if is_me { "flex-end" } else { "flex-start" })>
                                <div style=format!(
                                    "max-width: 80%; padding: 12px 16px; border-radius: {}; background: {}; color: {}; box-shadow: 0 1px 2px rgba(0,0,0,0.05);",
                                    if is_me { "18px 18px 2px 18px" } else { "18px 18px 18px 2px" },
                                    if is_me { "#007aff" } else { "#fff" },
                                    if is_me { "#fff" } else { "#111" }
                                )>
                                    <div style="white-space: pre-wrap; font-size: 0.95rem; line-height: 1.5;">{msg.content}</div>
                                </div>
                            </div>
                        }
                    }
                />
                
                <Show when=move || is_generating.get()>
                    <div style="display: flex; flex-direction: column; align-items: flex-start; animation: fadeIn 0.3s forwards;">
                        <div style="max-width: 80%; padding: 12px 16px; border-radius: 18px 18px 18px 2px; background: #fff; color: #111; box-shadow: 0 1px 2px rgba(0,0,0,0.05);">
                            <div style="white-space: pre-wrap; font-size: 0.95rem; line-height: 1.5;">{move || current_ai_msg.get()}</div>
                        </div>
                    </div>
                    
                    <div style="align-self: flex-start; padding: 6px 16px; color: #999; font-size: 0.8rem; margin-top: 4px;">
                        "로컬 연산 중"
                        <span style="animation: dotPulse 1.5s infinite;">"..."</span>
                    </div>
                </Show>
            </div>

            <footer style="padding: 16px 20px; border-top: 1px solid #eee; background: #fff;">
                <div style="max-width: 800px; margin: 0 auto; display: flex; gap: 10px; align-items: flex-end; background: #f4f4f4; padding: 6px 10px; border-radius: 20px;">
                    <textarea 
                        style="flex: 1; padding: 8px 10px; border: none; background: transparent; resize: none; max-height: 180px; font-size: 1rem; outline: none; line-height: 1.4;"
                        placeholder="메시지를 입력하세요 (오프라인 모드)..."
                        rows="1"
                        disabled=move || app_status.get() != AppStatus::Ready
                        prop:value=move || input_text.get()
                        on:input=move |ev| {
                            let target = event_target::<HtmlTextAreaElement>(&ev);
                            input_text.set(target.value());
                            
                            let style = web_sys::HtmlElement::style(&target);
                            let _ = style.set_property("height", "auto");
                            let _ = style.set_property("height", &format!("{}px", target.scroll_height()));
                        }
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" && !ev.shift_key() && !ev.is_composing() {
                                ev.prevent_default(); 
                                if !is_generating.get() && app_status.get() == AppStatus::Ready { send_message(); }
                            }
                        }
                    ></textarea>
                    
                    <Show when=move || !is_generating.get()>
                        <button 
                            style="width: 36px; height: 36px; border-radius: 50%; background: #000; color: #fff; border: none; cursor: pointer; display: flex; align-items: center; justify-content: center; font-weight: bold;"
                            disabled=move || app_status.get() != AppStatus::Ready
                            on:click=move |_| send_message()
                        >
                            "↑"
                        </button>
                    </Show>
                    
                    <Show when=move || is_generating.get()>
                        <button 
                            style="width: 36px; height: 36px; border-radius: 50%; background: #ff3b30; color: #fff; border: none; cursor: pointer; display: flex; align-items: center; justify-content: center;"
                            on:click=move |_| stop_generation()
                        >
                            "■"
                        </button>
                    </Show>
                </div>
                
                // 💡 추가된 AI 환각(Hallucination) 및 프라이버시 하단 경고 문구
                <div style="text-align: center; margin-top: 12px; font-size: 0.75rem; color: #888; line-height: 1.5;">
                    <span style="display: inline-block; margin-bottom: 4px;">"🔒 Privacy First: 귀하의 데이터는 기기 외부로 전송되지 않습니다."</span><br/>
                    <span style="color: #999;">"💡 AI는 부정확하거나 사실과 다른 정보를 생성할 수 있습니다. 중요한 정보는 반드시 교차 검증하시기 바랍니다."</span>
                </div>
            </footer>
        </main>
    }
}