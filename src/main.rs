mod app;
mod models;  // 🔥 추가됨
mod storage; // 🔥 추가됨

use app::*;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}