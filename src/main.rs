mod app; // [핵심] "같은 폴더에 있는 app.rs 파일을 이 프로젝트의 모듈로 연결할게!"

use app::{App, worker_main}; // [핵심] 프로젝트 이름 없이 바로 가져옵니다.
use wasm_bindgen::JsValue;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    
    let global = js_sys::global();
    let is_worker = js_sys::Reflect::has(&global, &JsValue::from_str("DedicatedWorkerGlobalScope")).unwrap_or(false);
    
    if is_worker {
        worker_main();
    } else {
        mount_to_body(App);
    }
}