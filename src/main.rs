mod app;
mod models;  // 🔥 추가됨
mod storage; // 🔥 추가됨
mod store; // 🔥 이 줄을 추가하세요!

use app::*;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}