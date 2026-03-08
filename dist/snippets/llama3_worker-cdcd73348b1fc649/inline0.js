
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
